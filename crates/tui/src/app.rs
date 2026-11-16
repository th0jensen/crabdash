//! Dashboard state, input and the terminal event loop.
use crate::{
    palette::Palette,
    poll::{self, Action, Request, Update},
    section::Section,
    touch::{Hits, Pointer},
    view,
};
use anyhow::Result;
use machines::resources::ResourceUsage;
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
use utils::{container::Container, disks::Disk, services::ServiceItem};

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
}

#[derive(Default)]
pub struct App {
    pub section: Section,
    pub host: Option<(String, String)>,
    pub failed: Option<String>,
    pub usage: Option<Result<ResourceUsage, String>>,
    pub cpu_history: VecDeque<u64>,
    pub docker: Option<Result<Vec<Container>, String>>,
    pub disks: Option<Result<Vec<Disk>, String>>,
    pub services: Option<Result<Vec<ServiceItem>, String>>,
    pub active_services: bool,
    pub tables: [TableState; 4],
    /// The selected container's or service's actions, open over the table.
    pub sheet: bool,
    pub logs: Option<Logs>,
    pub status: Option<Result<String, String>>,
    pub palette: Palette,
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
        .map_err(anyhow::Error::from)
        .and_then(|()| app.event_loop(&mut terminal, &requests, &updates));
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
            Update::Host { name, platform } => self.host = Some((name, platform)),
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
            Update::Docker(docker) => self.docker = Some(docker),
            Update::Disks(disks) => self.disks = Some(disks),
            Update::Services(services) => self.services = Some(services),
            Update::Logs { title, text } => {
                self.status = None;
                self.logs = Some(Logs {
                    title,
                    text,
                    offset: 0,
                    follow: true,
                    max: 0,
                })
            }
            Update::Done(result) => self.status = Some(result),
        }
        self.clamp_selection();
    }

    /// Services the table shows by name, like the desktop app, optionally active only.
    pub fn visible_services(&self) -> Vec<&ServiceItem> {
        let Some(Ok(services)) = &self.services else {
            return Vec::new();
        };
        let mut services: Vec<_> = services
            .iter()
            .filter(|service| !self.active_services || service.is_running())
            .collect();
        services.sort_by_cached_key(|service| service.name.to_lowercase());
        services
    }

    /// Rows in the current section's table.
    pub fn row_count(&self) -> usize {
        match self.section {
            Section::Docker => self
                .docker
                .as_ref()
                .and_then(|d| d.as_ref().ok())
                .map_or(0, Vec::len),
            Section::Disks => self
                .disks
                .as_ref()
                .and_then(|d| d.as_ref().ok())
                .map_or(0, |disks| view::disk_rows(disks).len()),
            Section::Services => self.visible_services().len(),
            Section::System => self
                .usage
                .as_ref()
                .and_then(|u| u.as_ref().ok())
                .and_then(|u| u.processes.as_ref())
                .map_or(0, Vec::len),
        }
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
            self.sheet = false;
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
        self.logs = None;
        self.sheet = false;
        self.status = None;
        self.clamp_selection();
    }

    pub fn selected_container(&self) -> Option<&Container> {
        let index = self.tables[Section::Docker.index()].selected()?;
        self.docker.as_ref()?.as_ref().ok()?.get(index)
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
        self.press(key.code, requests);
    }

    fn press(&mut self, code: KeyCode, requests: &Sender<Request>) {
        match code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc if self.logs.is_some() => self.logs = None,
            KeyCode::Esc if self.sheet => self.sheet = false,
            KeyCode::Esc => self.quit = true,
            KeyCode::Tab | KeyCode::Right => self.switch(self.section.step(1)),
            KeyCode::BackTab | KeyCode::Left => self.switch(self.section.step(-1)),
            KeyCode::Char(digit @ '1'..='4') => {
                self.switch(Section::ALL[digit as usize - '1' as usize]);
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Home => self.move_selection(isize::MIN / 2),
            KeyCode::End => self.move_selection(isize::MAX / 2),
            KeyCode::Char('a') if self.section == Section::Services => {
                self.active_services = !self.active_services;
                self.sheet = false;
                self.clamp_selection();
            }
            KeyCode::Enter if self.logs.is_none() && self.actionable() => {
                self.sheet = self.table().selected().is_some();
            }
            code if self.logs.is_none() => {
                if let Some((action, label)) = self.action(code) {
                    self.sheet = false;
                    self.status = Some(Ok(label));
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

    /// A tap on a button presses its key; on a tab switches to it; on a row
    /// selects it and, for containers and services, opens its actions.
    fn tap(&mut self, at: Position, requests: &Sender<Request>) {
        if let Some(code) = self.hits.key(at) {
            return self.press(code, requests);
        }
        if self.logs.is_some() {
            return;
        }
        if let Some(section) = self.hits.tab(at) {
            return self.switch(section);
        }
        match self.hits.row(at) {
            Some(index) if index < self.row_count() && self.selectable() => {
                self.table().select(Some(index));
                self.sheet = self.actionable();
            }
            // A tap outside the sheet closes it, as on a phone.
            _ if self.hits.overlay.is_none_or(|sheet| !sheet.contains(at)) => self.sheet = false,
            _ => {}
        }
    }
}
