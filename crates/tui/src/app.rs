//! Dashboard state, input and the terminal event loop.
use crate::{
    palette::Palette,
    poll::{self, Action, Request, Update},
    report::{Report, Reporter},
    search::{self, Search},
    section::Section,
    touch::{Hits, Pointer},
    view,
};
use anyhow::Result;
use machines::resources::{ProcessUsage, ResourceUsage};
use ratatui::{
    DefaultTerminal,
    crossterm::{
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
            KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        },
        execute,
    },
    layout::Position,
    widgets::TableState,
};
use services::{ServiceAction, docker::DockerAction};
use std::{
    collections::VecDeque,
    io::stdout,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};
use utils::{
    container::{Container, details::ContainerDetails},
    disks::Disk,
    services::ServiceItem,
};

const HISTORY: usize = 120;
const WHEEL_ROWS: isize = 3;

pub struct Logs {
    pub title: String,
    pub text: Result<String, String>,
    /// First line shown; ignored while following the newest line.
    pub offset: u16,
    pub follow: bool,
    /// The largest offset, from the last frame.
    pub max: u16,
    /// Shows only the lines with every word of it.
    pub query: String,
}

#[derive(Default)]
pub struct App {
    pub section: Section,
    pub failed: Option<String>,
    pub usage: Option<Result<ResourceUsage, String>>,
    pub cpu_history: VecDeque<u64>,
    pub docker: Option<Result<Vec<Container>, String>>,
    pub disks: Option<Result<Vec<Disk>, String>>,
    pub services: Option<Result<Vec<ServiceItem>, String>>,
    /// Each section's query and chip.
    pub search: [Search; 4],
    /// Keys go to the search field.
    pub typing: bool,
    pub tables: [TableState; 4],
    /// The selected container or service shows its details and actions
    /// under its row, and keeps doing so as the selection moves.
    pub expanded: bool,
    /// The last inspected container: its ID and details.
    pub details: Option<(String, Result<ContainerDetails, String>)>,
    inspecting: Option<String>,
    pub logs: Option<Logs>,
    pub status: Option<Result<String, String>>,
    /// An action or log request is running.
    acting: bool,
    reporter: Reporter,
    pub palette: Palette,
    /// Logs waiting for Bless, which temporarily takes the terminal.
    #[cfg(unix)]
    paging: Option<Logs>,
    pub hits: Hits,
    pointer: Pointer,
    quit: bool,
}

pub fn run(section: Section) -> Result<()> {
    let (requests, updates) = poll::spawn(section);
    let mut terminal = ratatui::init();
    let mut app = App {
        section,
        palette: Palette::query(),
        ..App::default()
    };
    let result = execute!(stdout(), EnableMouseCapture)
        .and_then(|()| Reporter::start(&mut stdout()))
        .map_err(anyhow::Error::from)
        .and_then(|()| app.event_loop(&mut terminal, &requests, &updates));
    let _ = Reporter::finish(&mut stdout());
    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

impl App {
    fn event_loop(
        &mut self,
        terminal: &mut DefaultTerminal,
        requests: &Sender<Request>,
        updates: &Receiver<Update>,
    ) -> Result<()> {
        while !self.quit {
            while let Ok(update) = updates.try_recv() {
                self.apply(update);
            }
            #[cfg(unix)]
            if let Some(logs) = self.paging.take() {
                self.page(terminal, logs)?;
            }
            self.inspect(requests);
            self.reporter.send(&mut stdout(), self.report())?;
            terminal.draw(|frame| view::draw(frame, self))?;
            if event::poll(Duration::from_millis(200))? {
                let previous = self.section;
                match event::read()? {
                    Event::Key(key) if key.kind != KeyEventKind::Release => self.key(key, requests),
                    Event::Mouse(mouse) => self.mouse(mouse, requests),
                    _ => {}
                }
                if self.section != previous {
                    let _ = requests.send(Request::Show(self.section));
                }
            }
        }
        Ok(())
    }

    fn apply(&mut self, update: Update) {
        match update {
            Update::Failed(error) => self.failed = Some(error),
            Update::Usage(usage) => {
                if let Ok(usage) = &usage {
                    let percent = usage.cpu_percent.unwrap_or_default().round() as u64;
                    if self.cpu_history.len() == HISTORY {
                        self.cpu_history.pop_front();
                    }
                    self.cpu_history.push_back(percent);
                }
                self.usage = Some(usage);
            }
            Update::Docker(docker) => {
                let selected = self.selected_container().map(|c| c.id.clone());
                self.docker = Some(docker);
                if let Some(id) = selected {
                    let index = self.visible_containers().iter().position(|c| c.id == id);
                    self.tables[Section::Docker.index()].select(index);
                    if index.is_none() && self.section == Section::Docker {
                        self.expanded = false;
                    }
                }
            }
            Update::Disks(disks) => self.disks = Some(disks),
            Update::Services(services) => {
                let selected = self.selected_service().map(|s| s.name.clone());
                self.services = Some(services);
                if let Some(name) = selected {
                    let index = self.visible_services().iter().position(|s| s.name == name);
                    self.tables[Section::Services.index()].select(index);
                    if index.is_none() && self.section == Section::Services {
                        self.expanded = false;
                    }
                }
            }
            Update::Logs { title, text } => {
                self.acting = false;
                self.status = None;
                let logs = Logs {
                    title,
                    text,
                    offset: 0,
                    follow: true,
                    max: 0,
                    query: String::new(),
                };
                #[cfg(unix)]
                if logs.text.as_ref().is_ok_and(|text| !text.trim().is_empty()) {
                    self.paging = Some(logs);
                    return;
                }
                self.logs = Some(logs);
            }
            Update::Details(id, details) => {
                self.inspecting = None;
                self.details = Some((id, details));
            }
            Update::Done(result) => {
                self.acting = false;
                self.details = None;
                self.status = Some(result);
            }
        }
        self.clamp_selection();
    }

    /// Bless owns raw mode and the alternate screen while its log viewer runs.
    #[cfg(unix)]
    fn page(&mut self, terminal: &mut DefaultTerminal, logs: Logs) -> Result<()> {
        execute!(stdout(), DisableMouseCapture)?;
        ratatui::restore();
        let shown = bless::page_text(&logs.title, logs.text.as_deref().unwrap_or_default());
        *terminal = ratatui::init();
        execute!(stdout(), EnableMouseCapture)?;
        if let Err(error) = shown {
            self.status = Some(Err(format!("Couldn't open Bless: {error:#}")));
            self.logs = Some(logs);
        }
        self.pointer = Pointer::default();
        self.reporter = Reporter::default();
        Ok(())
    }

    /// Asks for the expanded container's details once, when they are not
    /// already shown or on their way.
    fn inspect(&mut self, requests: &Sender<Request>) {
        if !self.expanded || self.section != Section::Docker || self.inspecting.is_some() {
            return;
        }
        let Some(id) = self.selected_container().map(|c| c.id.clone()) else {
            return;
        };
        let shown = self.details.as_ref().is_some_and(|(shown, _)| *shown == id);
        if !shown && self.inspecting.as_ref() != Some(&id) {
            self.inspecting = Some(id.clone());
            let _ = requests.send(Request::Inspect(id));
        }
    }

    /// Opens or closes the selected row's details, where rows have actions.
    fn toggle(&mut self) {
        if self.logs.is_none() && self.actionable() {
            self.expanded = !self.expanded && self.table().selected().is_some();
            if self.expanded {
                self.details = None;
            }
        }
    }

    /// The title and status Rex shows for this pane: working with the
    /// running action, then at rest with any failure.
    fn report(&self) -> Report {
        let msg = match &self.status {
            Some(Ok(label)) if self.acting => Some(label.clone()),
            Some(Err(error)) => Some(error.clone()),
            _ => None,
        };
        Report {
            title: format!("Crabdash · {}", self.section.title()),
            working: self.acting,
            msg,
        }
    }

    fn current_search(&self, section: Section) -> &Search {
        &self.search[section.index()]
    }

    /// Containers matching the query and the running or stopped chip.
    pub fn visible_containers(&self) -> Vec<&Container> {
        let Some(Ok(containers)) = &self.docker else {
            return Vec::new();
        };
        let search = self.current_search(Section::Docker);
        containers
            .iter()
            .filter(|c| match search.chip {
                1 => c.is_active_status(),
                2 => !c.is_active_status(),
                _ => true,
            })
            .filter(|c| search::matches(&search.query, &[&c.name, &c.status, &c.id]))
            .collect()
    }

    /// Services by name, like the desktop app, matching the query and chip.
    pub fn visible_services(&self) -> Vec<&ServiceItem> {
        let Some(Ok(services)) = &self.services else {
            return Vec::new();
        };
        let search = self.current_search(Section::Services);
        let mut services: Vec<_> = services
            .iter()
            .filter(|service| match search.chip {
                1 => service.is_running(),
                2 => service.status_label() == "Failed",
                _ => true,
            })
            .filter(|service| {
                let description = service.description.as_deref().unwrap_or_default();
                search::matches(&search.query, &[&service.name, description])
            })
            .collect();
        services.sort_by_cached_key(|service| service.name.to_lowercase());
        services
    }

    /// Disks with a partition, mount or name matching the query, whole.
    pub fn visible_disks(&self) -> Vec<&Disk> {
        let Some(Ok(disks)) = &self.disks else {
            return Vec::new();
        };
        let search = self.current_search(Section::Disks);
        disks
            .iter()
            .filter(|disk| match search.chip {
                1 => disk.is_healthy(),
                2 => !disk.is_healthy(),
                _ => true,
            })
            .filter(|disk| {
                view::disk_rows(&[disk]).iter().any(|(_, name, _, detail)| {
                    search::matches(&search.query, &[name, detail.unwrap_or_default()])
                })
            })
            .collect()
    }

    /// Processes matching the query, in the chip's order.
    pub fn visible_processes(&self) -> Vec<ProcessUsage> {
        let Some(Ok(usage)) = &self.usage else {
            return Vec::new();
        };
        let search = self.current_search(Section::System);
        let mut processes: Vec<_> = usage
            .processes
            .iter()
            .flatten()
            .filter(|p| {
                let pid = p.pid.to_string();
                let user = p.user.as_deref().unwrap_or_default();
                search::matches(&search.query, &[&p.name, user, &pid])
            })
            .cloned()
            .collect();
        match search.chip {
            1 => processes.sort_by_key(|p| std::cmp::Reverse(p.memory_bytes)),
            2 => processes.sort_by_cached_key(|p| p.name.to_lowercase()),
            _ => processes.sort_by(|a, b| {
                b.cpu_percent
                    .unwrap_or(-1.0)
                    .total_cmp(&a.cpu_percent.unwrap_or(-1.0))
            }),
        }
        processes
    }

    /// Rows in the current section's table.
    pub fn row_count(&self) -> usize {
        match self.section {
            Section::Docker => self.visible_containers().len(),
            Section::Disks => view::disk_rows(&self.visible_disks()).len(),
            Section::Services => self.visible_services().len(),
            Section::System => self.visible_processes().len(),
        }
    }

    /// The query the search field edits: the logs' while they are open.
    pub fn query(&self) -> &str {
        match &self.logs {
            Some(logs) => &logs.query,
            None => &self.current_search(self.section).query,
        }
    }

    fn query_mut(&mut self) -> &mut String {
        match &mut self.logs {
            Some(logs) => &mut logs.query,
            None => &mut self.search[self.section.index()].query,
        }
    }

    /// After the query or chip changes, the list starts again from its top.
    fn refilter(&mut self) {
        if let Some(logs) = &mut self.logs {
            logs.follow = true;
            return;
        }
        self.expanded = false;
        let table = self.table();
        *table.offset_mut() = 0;
        table.select(None);
        self.clamp_selection();
    }

    fn choose_chip(&mut self, chip: usize) {
        let chips = search::chips(self.section).len();
        if self.logs.is_some() || chips == 0 {
            return;
        }
        self.search[self.section.index()].chip = chip % chips;
        self.refilter();
    }

    /// The system view's processes scroll but are not selected.
    pub fn selectable(&self) -> bool {
        self.section != Section::System
    }

    /// Containers and services have actions; tapping one opens them.
    fn actionable(&self) -> bool {
        matches!(self.section, Section::Docker | Section::Services)
    }

    fn table(&mut self) -> &mut TableState {
        &mut self.tables[self.section.index()]
    }

    fn clamp_selection(&mut self) {
        let count = self.row_count();
        let selectable = self.selectable();
        let table = self.table();
        match (count, table.selected()) {
            (0, _) => table.select(None),
            _ if !selectable => table.select(None),
            (_, None) => table.select(Some(0)),
            (count, Some(index)) if index >= count => table.select(Some(count - 1)),
            _ => {}
        }
        if self.table().selected().is_none() {
            self.expanded = false;
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.logs.is_some() || !self.selectable() {
            return self.scroll(delta);
        }
        let count = self.row_count();
        if count == 0 {
            return;
        }
        let table = self.table();
        let index = table.selected().unwrap_or_default() as isize + delta;
        table.select(Some(index.clamp(0, count as isize - 1) as usize));
    }

    /// Scrolls the logs or the table's view, as a scrollbox would. A table's
    /// selection moves only as far as it must to stay in view.
    fn scroll(&mut self, delta: isize) {
        if let Some(logs) = &mut self.logs {
            let current = if logs.follow { logs.max } else { logs.offset };
            logs.offset = (current as isize + delta).clamp(0, logs.max as isize) as u16;
            logs.follow = logs.offset == logs.max;
            return;
        }
        let count = self.row_count();
        let visible = self.hits.visible_rows();
        let table = self.table();
        let last = count.saturating_sub(visible);
        let offset = (table.offset() as isize + delta).clamp(0, last as isize) as usize;
        *table.offset_mut() = offset;
        if let Some(selected) = table.selected() {
            let bottom = (offset + visible).min(count).saturating_sub(1);
            table.select(Some(selected.clamp(offset, bottom.max(offset))));
        }
    }

    fn switch(&mut self, section: Section) {
        self.section = section;
        self.typing = false;
        self.logs = None;
        self.expanded = false;
        self.status = None;
        self.clamp_selection();
    }

    pub fn selected_container(&self) -> Option<&Container> {
        let index = self.tables[Section::Docker.index()].selected()?;
        self.visible_containers().get(index).copied()
    }

    pub fn selected_service(&self) -> Option<&ServiceItem> {
        let index = self.tables[Section::Services.index()].selected()?;
        self.visible_services().get(index).copied()
    }

    /// The action a key performs on the selected container or service, with
    /// what the status line says while it runs.
    fn action(&self, key: KeyCode) -> Option<(Action, String)> {
        match self.section {
            Section::Docker => {
                let container = self.selected_container()?;
                let id = container.id.clone();
                let action = match key {
                    KeyCode::Char('s') if container.is_active_status() => DockerAction::Stop,
                    KeyCode::Char('s') => DockerAction::Start,
                    KeyCode::Char('r') => DockerAction::Restart,
                    KeyCode::Char('p') if container.is_paused() => DockerAction::Unpause,
                    KeyCode::Char('p') if container.is_running_status() => DockerAction::Pause,
                    KeyCode::Char('l') => {
                        let name = container.name.clone();
                        return Some((Action::DockerLogs(id, name), "Loading logs…".into()));
                    }
                    _ => return None,
                };
                let label = format!("{} {}…", action.pending_label(), container.name);
                Some((Action::Docker(id, action), label))
            }
            Section::Services => {
                let service = self.selected_service()?;
                let name = service.name.clone();
                let action = match key {
                    KeyCode::Char('s') if service.is_running() => ServiceAction::Stop,
                    KeyCode::Char('s') => ServiceAction::Start,
                    KeyCode::Char('r') => ServiceAction::Restart,
                    KeyCode::Char('l') => {
                        return Some((Action::ServiceLogs(name), "Loading logs…".into()));
                    }
                    _ => return None,
                };
                let label = format!("{} {name}…", action.pending_label());
                Some((Action::Service(name, action), label))
            }
            Section::Disks | Section::System => None,
        }
    }

    fn key(&mut self, key: KeyEvent, requests: &Sender<Request>) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.typing && self.edit(key) {
            return;
        }
        self.press(key.code, requests);
    }

    /// Edits the search field while it has the keys. Keys that move or
    /// switch, such as arrows and Tab, fall through to the dashboard.
    fn edit(&mut self, key: KeyEvent) -> bool {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('u') if control => self.query_mut().clear(),
            KeyCode::Char(c) if !control => self.query_mut().push(c),
            KeyCode::Backspace => {
                self.query_mut().pop();
            }
            KeyCode::Enter | KeyCode::Esc => {
                self.typing = false;
                return true;
            }
            _ => {
                self.typing = false;
                return false;
            }
        }
        self.refilter();
        true
    }

    fn press(&mut self, code: KeyCode, requests: &Sender<Request>) {
        match code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('/') => self.typing = true,
            // The search field's × clears it.
            KeyCode::Delete => {
                self.query_mut().clear();
                self.typing = false;
                self.refilter();
            }
            KeyCode::Esc if !self.query().is_empty() => {
                self.query_mut().clear();
                self.refilter();
            }
            KeyCode::Esc if self.logs.is_some() => self.logs = None,
            KeyCode::Esc if self.expanded => self.expanded = false,
            KeyCode::Esc => self.quit = true,
            KeyCode::Tab | KeyCode::Enter => self.toggle(),
            KeyCode::Right | KeyCode::Char(']') => self.switch(self.section.step(1)),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('[') => {
                self.switch(self.section.step(-1));
            }
            KeyCode::Char(digit @ '1'..='4') => {
                self.switch(Section::ALL[digit as usize - '1' as usize]);
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Home => self.move_selection(isize::MIN / 2),
            KeyCode::End => self.move_selection(isize::MAX / 2),
            KeyCode::Char('f') => self.choose_chip(self.search[self.section.index()].chip + 1),
            KeyCode::Char('F') => {
                let chips = search::chips(self.section).len().max(1);
                self.choose_chip(self.search[self.section.index()].chip + chips - 1);
            }
            code if self.logs.is_none() && !self.acting => {
                if let Some((action, label)) = self.action(code) {
                    self.status = Some(Ok(label));
                    self.acting = true;
                    let _ = requests.send(Request::Act(action));
                }
            }
            _ => {}
        }
    }

    fn mouse(&mut self, mouse: MouseEvent, requests: &Sender<Request>) {
        let at = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll(-WHEEL_ROWS),
            MouseEventKind::ScrollDown => self.scroll(WHEEL_ROWS),
            MouseEventKind::Down(MouseButton::Left) => self.pointer.press(at),
            MouseEventKind::Drag(MouseButton::Left) => self.pointer.drag(),
            MouseEventKind::Up(MouseButton::Left) if self.pointer.release(at) => {
                self.tap(at, requests);
            }
            _ => {}
        }
    }

    /// A tap on a button presses its key; on a tab or chip chooses it; on the
    /// search field gives it the keys; on a row selects it and, for containers
    /// and services, opens its actions.
    fn tap(&mut self, at: Position, requests: &Sender<Request>) {
        if let Some(code) = self.hits.key(at) {
            return self.press(code, requests);
        }
        let field = self.hits.search.is_some_and(|field| field.contains(at));
        self.typing = field;
        if field {
            return;
        }
        if let Some(section) = self.hits.tab(at) {
            return self.switch(section);
        }
        if let Some(chip) = self.hits.chip(at) {
            return self.choose_chip(chip);
        }
        if self.logs.is_some() {
            return;
        }
        match self.hits.row(at) {
            // A tap on the expanded row closes it; on another row, moves the
            // details there.
            Some(index) if index < self.row_count() && self.selectable() => {
                let again = self.table().selected() == Some(index) && self.expanded;
                self.table().select(Some(index));
                self.expanded = !again && self.actionable();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use std::sync::mpsc::channel;

    fn container(id: &str, name: &str, status: &str) -> Container {
        Container {
            id: id.into(),
            name: name.into(),
            status: status.into(),
            ..Container::default()
        }
    }

    #[test]
    fn filtered_actions_keep_the_container_identity_across_refreshes() {
        let (tx, rx) = channel();
        let mut app = App::default();
        app.apply(Update::Docker(Ok(vec![
            container("database", "db", "running"),
            container("web-live", "Web", "running"),
            container("web-old", "web-old", "exited"),
        ])));
        app.press(KeyCode::Char('/'), &tx);
        for c in "wEb".chars() {
            app.key(KeyCode::Char(c).into(), &tx);
        }
        app.key(KeyCode::Enter.into(), &tx);
        app.choose_chip(1);
        assert_eq!(app.row_count(), 1);
        app.press(KeyCode::Tab, &tx);
        assert!(app.expanded);
        app.apply(Update::Docker(Ok(vec![
            container("new", "web-new", "running"),
            container("web-live", "Web", "running"),
        ])));
        app.press(KeyCode::Char('s'), &tx);
        assert!(matches!(
            rx.try_recv(),
            Ok(Request::Act(Action::Docker(id, DockerAction::Stop))) if id == "web-live"
        ));
        app.apply(Update::Docker(Ok(vec![container(
            "new", "web-new", "running",
        )])));
        assert!(!app.expanded);
    }

    #[test]
    fn expanded_actions_remain_visible_and_tappable_after_resizing() -> Result<()> {
        let (tx, rx) = channel();
        let mut app = App::default();
        app.apply(Update::Docker(Ok(vec![container("web", "web", "running")])));
        app.press(KeyCode::Tab, &tx);
        app.details = Some((
            "web".into(),
            Ok(ContainerDetails {
                id: "web".into(),
                name: "web".into(),
                image: Some(format!(
                    "registry.example.com/team/{}/latest",
                    "long-name".repeat(8)
                )),
                status: Some("running".into()),
                health: None,
                restart_policy: None,
                ports: None,
                mounts: None,
                created: None,
            }),
        ));
        let mut terminal = Terminal::new(TestBackend::new(130, 24))?;
        for (width, height) in [(130, 24), (46, 30), (46, 12), (130, 24)] {
            terminal.backend_mut().resize(width, height);
            terminal.draw(|frame| view::draw(frame, &mut app))?;
            let screen: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            for label in ["stop", "restart", "pause", "logs"] {
                assert!(
                    screen.contains(label),
                    "{label} missing at {width}×{height}"
                );
            }
            let &(target, _) = app
                .hits
                .keys
                .iter()
                .find(|(_, key)| *key == KeyCode::Char('p'))
                .ok_or_else(|| anyhow::anyhow!("pause has no tap target"))?;
            app.tap(Position::new(target.x, target.bottom() - 1), &tx);
            assert!(matches!(
                rx.try_recv(),
                Ok(Request::Act(Action::Docker(id, DockerAction::Pause))) if id == "web"
            ));
            app.acting = false;
            app.status = None;
        }
        Ok(())
    }
}
