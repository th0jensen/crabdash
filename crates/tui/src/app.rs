//! Dashboard state, input and the terminal event loop.
use crate::{
    poll::{self, Action, Request, Update},
    section::Section,
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
    layout::{Position, Rect},
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

pub struct Logs {
    pub title: String,
    pub text: Result<String, String>,
    pub scroll: u16,
}

/// Screen regions the last frame drew, so taps from touch clients such as Rex
/// for iOS can switch tabs, pick rows and press the key hints.
#[derive(Default)]
pub struct Hits {
    pub tabs: Vec<(Rect, Section)>,
    pub keys: Vec<(Rect, KeyCode)>,
    /// The table body and the index of its first visible row.
    pub rows: Option<(Rect, usize)>,
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
    pub logs: Option<Logs>,
    pub status: Option<Result<String, String>>,
    pub hits: Hits,
    quit: bool,
}

pub fn run(section: Section) -> Result<()> {
    let (requests, updates) = poll::spawn(section);
    let mut app = App {
        section,
        ..App::default()
    };
    let mut terminal = ratatui::init();
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
                    scroll: 0,
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

    /// Rows in the current section's table; the system view has none to select.
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
            Section::System => 0,
        }
    }

    fn table(&mut self) -> &mut TableState {
        &mut self.tables[self.section.index()]
    }

    fn clamp_selection(&mut self) {
        let count = self.row_count();
        let table = self.table();
        match (count, table.selected()) {
            (0, _) => table.select(None),
            (_, None) => table.select(Some(0)),
            (count, Some(index)) if index >= count => table.select(Some(count - 1)),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if let Some(logs) = &mut self.logs {
            logs.scroll = logs.scroll.saturating_add_signed(delta as i16);
            return;
        }
        let count = self.row_count();
        if count == 0 {
            return;
        }
        let table = self.table();
        let index = table.selected().unwrap_or_default() as isize + delta;
        table.select(Some(index.clamp(0, count as isize - 1) as usize));
    }

    fn switch(&mut self, section: Section) {
        self.section = section;
        self.logs = None;
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
    /// what the footer says while it runs.
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
                    KeyCode::Char('l') | KeyCode::Enter => {
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
                    KeyCode::Char('l') | KeyCode::Enter => {
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
                self.clamp_selection();
            }
            code if self.logs.is_none() => {
                if let Some((action, label)) = self.action(code) {
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
            MouseEventKind::ScrollUp => self.move_selection(-3),
            MouseEventKind::ScrollDown => self.move_selection(3),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, section)) = self.hits.tabs.iter().find(|(r, _)| r.contains(at)) {
                    self.switch(*section);
                } else if let Some((_, code)) = self.hits.keys.iter().find(|(r, _)| r.contains(at))
                {
                    self.press(*code, requests);
                } else if let Some((body, offset)) = self.hits.rows
                    && body.contains(at)
                    && self.logs.is_none()
                {
                    let index = offset + usize::from(at.y - body.y);
                    if index < self.row_count() {
                        self.table().select(Some(index));
                    }
                }
            }
            _ => {}
        }
    }
}
