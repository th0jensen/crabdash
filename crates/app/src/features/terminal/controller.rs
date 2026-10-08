use crate::components::terminal_input::{TerminalInput, TerminalInputEvent};
use crate::{app::Crabdash, features::terminal};
use gpui::*;
use machines::terminal::TerminalEvent;

impl Crabdash {
    /// A store reload that retains the selected UUID must not refocus a shell.
    pub(crate) fn reconcile_selected_terminal(
        &mut self,
        previous: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_machine().uuid == previous {
            return;
        }
        self.focus_selected_terminal(window, cx);
    }

    pub(crate) fn focus_selected_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quake_terminal_open {
            self.open_quake_terminal(window, cx);
        } else {
            window.focus(&self.focus_handle);
        }
    }
    pub(crate) fn active_quake_terminal(&self) -> Option<&terminal::QuakeTerminal> {
        self.quake_terminals
            .get(&self.selected_machine().uuid)?
            .active()
    }

    pub(crate) fn set_quake_terminal_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if open {
            self.open_quake_terminal(window, cx);
        } else {
            self.close_quake_terminal(window, cx);
        }
        self.persist_workspace(cx);
    }

    pub(crate) fn toggle_quake_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        if self.quake_terminal_open {
            self.close_quake_terminal(window, cx);
        } else {
            self.open_quake_terminal(window, cx);
        }
        self.persist_workspace(cx);
    }

    fn quake_geometry(&self, window: &Window, cx: &App) -> terminal::geometry::Geometry {
        let (cell_width, cell_height) = terminal::cell_metrics(&self.preferences, cx);
        let (width, height) = terminal::panes::viewport(window);
        let scale = f32::from(window.rem_size()) / 16.0;
        let titlebar = if crate::desktop::shell::is_native(self) {
            0.0
        } else {
            crate::components::style::TITLE_BAR * scale
        };
        let reserve = (terminal::geometry::DASHBOARD_RESERVE * scale).max(
            titlebar
                + crate::content::minimum_visible_height(&self.workspaces.layout().root, scale),
        );
        terminal::geometry::Geometry::new(
            width,
            height,
            f32::from(window.rem_size()) * crate::components::style::BAR / 16.0,
            cell_width,
            cell_height,
        )
        .with_dashboard_reserve(reserve)
    }

    fn quake_pane_sizing(
        &self,
        width: f32,
        height: f32,
        window: &Window,
        cx: &App,
    ) -> terminal::sizing::Sizing {
        let (cell_width, cell_height) = terminal::cell_metrics(&self.preferences, cx);
        terminal::geometry::Geometry::new(
            width,
            height,
            f32::from(window.rem_size()) * crate::components::style::BAR / 16.0,
            cell_width,
            cell_height,
        )
        .pane_sizing(height, window.scale_factor())
    }

    fn quake_pane_metrics(&self, window: &Window, cx: &App) -> terminal::panes::Metrics {
        terminal::panes::Metrics::new(
            f32::from(window.rem_size()) / 16.0,
            terminal::cell_metrics(&self.preferences, cx).1,
        )
    }

    fn clamp_quake_height(&self, requested: f32, window: &Window, cx: &App) -> f32 {
        let metrics = self.quake_pane_metrics(window, cx);
        let node = self
            .quake_terminals
            .get(&self.selected_machine().uuid)
            .map(|drawer| &drawer.model.layout.root);
        terminal::panes::clamp_height(self.quake_geometry(window, cx), requested, node, metrics)
    }

    pub(super) fn terminal_neighbor(
        &self,
        direction: terminal::panes::Direction,
        window: &Window,
        cx: &App,
    ) -> Option<uuid::Uuid> {
        let drawer = self.quake_terminals.get(&self.selected_machine().uuid)?;
        let metrics = self.quake_pane_metrics(window, cx);
        let height = self.clamp_quake_height(self.requested_quake_height(window, cx), window, cx);
        let rects = terminal::panes::rectangles(
            &drawer.model.layout.root,
            terminal::panes::viewport(window).0,
            (height - terminal::geometry::PANEL_BORDER).max(0.0),
            metrics,
        );
        terminal::panes::neighbor(&rects, drawer.model.layout.focused, direction)
    }

    /// Retain smooth logical pixels; only the emulation grid uses whole cells.
    pub(crate) fn set_quake_height(
        &mut self,
        height: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let height = px(self.clamp_quake_height(f32::from(height), window, cx));
        if let Some(draft) = &mut self.quake_resize {
            // Dragging chooses the visible height, not off-window overshoot.
            draft.height = f32::from(height);
        }
        if self.quake_height == height {
            return;
        }
        self.quake_height = height;
        self.resize_quake_terminal(window, cx);
        cx.notify();
    }

    pub(crate) fn resize_quake_terminal(&mut self, window: &Window, cx: &mut App) {
        // Window and font changes can reduce available space without a drag.
        self.quake_height =
            px(self.clamp_quake_height(self.requested_quake_height(window, cx), window, cx));
        if !self.quake_terminal_open {
            return;
        }
        let machine = self.selected_machine().uuid;
        let Some(drawer) = self.quake_terminals.get(&machine) else {
            return;
        };
        let visible = terminal::panes::visible(
            &drawer.model.layout.root,
            terminal::panes::viewport(window).0,
            (f32::from(self.quake_height) - terminal::geometry::PANEL_BORDER).max(0.0),
            self.quake_pane_metrics(window, cx),
        );
        for (session, width, height) in visible {
            let sizing = self.quake_pane_sizing(width, height, window, cx);
            let Some(quake) = self
                .quake_terminals
                .get_mut(&machine)
                .and_then(|drawer| drawer.sessions.get_mut(&session))
            else {
                continue;
            };
            let (resize_emulator, resize_pty) = sizing.changes(quake.emulator_size, quake.size);
            if resize_emulator {
                if let Err(error) = quake.terminal.resize(
                    sizing.emulator.columns,
                    sizing.emulator.rows,
                    sizing.emulator.cell_width,
                    sizing.emulator.cell_height,
                ) {
                    quake.status = terminal::QuakeTerminalStatus::Failed;
                    quake
                        .input
                        .update(cx, |input, cx| input.set_connected(false, cx));
                    tracing::warn!(%error, "Failed to resize Ghostty terminal");
                    continue;
                }
                quake.emulator_size = sizing.emulator;
                if let Some(controller) = &quake.controller {
                    for response in quake.terminal.take_pty_writes() {
                        if let Err(error) = controller.write(response) {
                            quake.status = terminal::QuakeTerminalStatus::Failed;
                            quake
                                .input
                                .update(cx, |input, cx| input.set_connected(false, cx));
                            tracing::warn!(%error, "Failed to send Ghostty resize response");
                        }
                    }
                }
            }
            if resize_pty {
                if let Some(controller) = quake.controller.as_ref()
                    && let Err(error) = controller.resize(sizing.pty)
                {
                    quake.status = terminal::QuakeTerminalStatus::Failed;
                    quake
                        .input
                        .update(cx, |input, cx| input.set_connected(false, cx));
                    tracing::warn!(%error, "Failed to resize terminal PTY");
                    continue;
                }
                quake.size = sizing.pty;
            }
        }
    }

    pub(crate) fn open_quake_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quake_terminal_open = true;
        if let Some(quake) = self.active_quake_terminal() {
            window.focus(&quake.input.focus_handle(cx));
            self.resize_quake_terminal(window, cx);
            cx.notify();
            return;
        }

        self.new_quake_terminal(window, cx);
    }

    pub(crate) fn new_quake_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.create_quake_terminal(None, window, cx);
    }

    pub(super) fn split_quake_terminal(
        &mut self,
        edge: crate::layout::Drop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.create_quake_terminal(Some(edge), window, cx);
    }

    fn create_quake_terminal(
        &mut self,
        edge: Option<crate::layout::Drop>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let machine = self.selected_machine().clone();
        let machine_uuid = machine.uuid;
        let session_uuid = uuid::Uuid::new_v4();
        let candidate = match self.quake_terminals.get(&machine_uuid) {
            Some(drawer) => drawer.model.with_session(session_uuid, edge),
            None if edge.is_none() => Some(terminal::model::Model::new(session_uuid)),
            None => None,
        };
        let Some(candidate) = candidate else {
            return;
        };
        let height = terminal::panes::clamp_height(
            self.quake_geometry(window, cx),
            self.requested_quake_height(window, cx),
            Some(&candidate.layout.root),
            self.quake_pane_metrics(window, cx),
        );
        let endpoint = machine
            .remote
            .as_ref()
            .map(|remote| format!("{}@{}", remote.user, remote.host))
            .unwrap_or_else(|| {
                let key = if cfg!(windows) { "USERNAME" } else { "USER" };
                let user = std::env::var(key)
                    .ok()
                    .filter(|user| !user.trim().is_empty())
                    .unwrap_or_else(|| "local".to_string());
                format!("{user}@{}", machine.system_info.machine_name)
            });
        let width = terminal::panes::viewport(window).0;
        let body_height = (height - terminal::geometry::PANEL_BORDER).max(0.0);
        let Some((_, width, pane_height)) = terminal::panes::visible(
            &candidate.layout.root,
            width,
            body_height,
            self.quake_pane_metrics(window, cx),
        )
        .into_iter()
        .find(|(id, _, _)| *id == session_uuid) else {
            return;
        };
        let sizing = self.quake_pane_sizing(width, pane_height, window, cx);
        let size = sizing.pty;
        let terminal = match terminal::TerminalState::new_interactive(
            size.columns,
            size.rows,
            self.preferences.scrollback_lines,
            sizing.emulator.cell_width,
            sizing.emulator.cell_height,
        ) {
            Ok(terminal) => terminal,
            Err(error) => {
                self.set_status_error(format!("Failed to initialise Ghostty terminal: {error}"));
                cx.notify();
                return;
            }
        };
        let input = cx.new(TerminalInput::new);
        let input_id = input.entity_id();
        let input_events = cx.subscribe(&input, move |this, _, event: &TerminalInputEvent, cx| {
            let Some(quake) = this
                .quake_terminals
                .get_mut(&machine_uuid)
                .and_then(|drawer| drawer.sessions.get_mut(&session_uuid))
                .filter(|quake| quake.input.entity_id() == input_id)
            else {
                return;
            };
            let data = match event {
                TerminalInputEvent::Input(bytes) => quake.terminal.prepare_input(bytes),
                TerminalInputEvent::Paste(text) => quake.terminal.prepare_paste(text),
                TerminalInputEvent::StartupOverflow => {
                    let message = "Terminal startup input exceeded its limit and was discarded. Retype after connection.";
                    quake.terminal.feed_string(format!("\r\n[crabdash] {message}\r\n"));
                    this.set_status_error(message.to_owned());
                    cx.notify();
                    return;
                }
                TerminalInputEvent::Scroll(scroll) => {
                    quake.terminal.scroll(*scroll);
                    cx.notify();
                    return;
                }
                TerminalInputEvent::Page(direction) => {
                    quake
                        .terminal
                        .scroll(libghostty_vt::terminal::ScrollViewport::Delta(
                            direction
                                * isize::try_from(quake.size.rows.saturating_sub(1)).unwrap_or(1),
                        ));
                    cx.notify();
                    return;
                }
            };
            if let Some(controller) = &quake.controller {
                if let Err(error) = data.and_then(|data| controller.write(data)) {
                    tracing::warn!(%error, "Failed to write terminal input");
                    this.set_status_error(format!("Unable to send terminal input: {error}"));
                }
            }
            cx.notify();
        });
        let session = terminal::QuakeTerminal {
            endpoint,
            custom_name: None,
            rename: None,
            terminal,
            controller: None,
            size,
            emulator_size: sizing.emulator,
            status: terminal::QuakeTerminalStatus::Connecting,
            input: input.clone(),
            _input_events: input_events,
        };
        if let Some(drawer) = self.quake_terminals.get_mut(&machine_uuid) {
            drawer.model = candidate;
            drawer.sessions.insert(session_uuid, session);
        } else {
            self.quake_terminals.insert(
                machine_uuid,
                terminal::Drawer {
                    model: candidate,
                    sessions: std::collections::HashMap::from([(session_uuid, session)]),
                },
            );
        }
        self.quake_terminal_open = true;
        self.quake_height = px(height);
        window.focus(&input.focus_handle(cx));
        self.resize_quake_terminal(window, cx);
        cx.notify();

        let options = self.preferences.terminal_options();
        cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = cx
                    .background_spawn({
                        let mut machine = machine;
                        async move { machine.open_terminal(size, options).await }
                    })
                    .await;

                let session = match result {
                    Ok(session) => session,
                    Err(error) => {
                        this.update(&mut cx, move |this, cx| {
                            if let Some(quake) = this.quake_terminals.get_mut(&machine_uuid).and_then(|drawer| drawer.sessions.get_mut(&session_uuid)).filter(|quake| quake.input.entity_id() == input_id) {
                                quake.status = terminal::QuakeTerminalStatus::Failed;
                                quake.input.update(cx, |input, cx| input.set_connected(false, cx));
                                quake.terminal.feed_string(format!(
                                    "\r\n[crabdash] Failed to open terminal: {error}\r\n"
                                ));
                            }
                            cx.notify();
                        })
                        .ok();
                        return;
                    }
                };

                let controller = session.controller;
                let events = session.events;
                let accepted = this
                    .update(&mut cx, |this, cx| {
                        let Some(quake) = this.quake_terminals.get_mut(&machine_uuid).and_then(|drawer| drawer.sessions.get_mut(&session_uuid)).filter(|quake| quake.input.entity_id() == input_id) else {
                            return false;
                        };
                        if quake.status == terminal::QuakeTerminalStatus::Failed {
                            quake.input.update(cx, |input, cx| input.set_connected(false, cx));
                            return false;
                        }
                        if quake.size != size
                            && let Err(error) = controller.resize(quake.size)
                        {
                            quake.status = terminal::QuakeTerminalStatus::Failed;
                            quake.input.update(cx, |input, cx| input.set_connected(false, cx));
                            quake.terminal.feed_string(format!(
                                "\r\n[crabdash] Failed to apply terminal size: {error}\r\n"
                            ));
                            tracing::warn!(%error, "Failed to apply current terminal size");
                            cx.notify();
                            return false;
                        }
                        quake.controller = Some(controller.clone());
                        for response in quake.terminal.take_pty_writes() {
                            if let Err(error) = controller.write(response) {
                                tracing::warn!(%error, "Failed to send pending Ghostty response");
                            }
                        }
                        quake.input.update(cx, |input, cx| {
                            input.set_connected(true, cx);
                        });
                        quake.status = terminal::QuakeTerminalStatus::Connected;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);

                if !accepted {
                    if let Err(error) = controller.shutdown() {
                        tracing::debug!(%error, "Failed to shut down unclaimed terminal");
                    }
                    return;
                }

                while let Ok(event) = events.recv().await {
                    let should_continue = this
                        .update(&mut cx, |this, cx| {
                            let Some(quake) = this.quake_terminals.get_mut(&machine_uuid).and_then(|drawer| drawer.sessions.get_mut(&session_uuid)).filter(|quake| quake.input.entity_id() == input_id) else {
                                return false;
                            };

                            match event {
                                TerminalEvent::Output(output) => {
                                    quake.terminal.feed(output);
                                    for response in quake.terminal.take_pty_writes() {
                                        if let Err(error) = controller.write(response) {
                                            quake.status =
                                                terminal::QuakeTerminalStatus::Failed;
                                            quake.input.update(cx, |input, cx| input.set_connected(false, cx));
                                            tracing::warn!(%error, "Failed to send Ghostty PTY response");
                                        }
                                    }
                                }
                                TerminalEvent::Exited(status) => {
                                    quake.controller = None;
                                    quake.input.update(cx, |input, cx| {
                                        input.set_connected(false, cx);
                                    });
                                    quake.status = terminal::QuakeTerminalStatus::Exited;
                                    quake.terminal.feed_string(format!(
                                        "\r\n[process exited{}]\r\n",
                                        status
                                            .map(|status| format!(" with status {status}"))
                                            .unwrap_or_default()
                                    ));
                                }
                                TerminalEvent::Error(error) => {
                                    quake.controller = None;
                                    quake.input.update(cx, |input, cx| {
                                        input.set_connected(false, cx);
                                    });
                                    quake.status = terminal::QuakeTerminalStatus::Failed;
                                    quake.terminal.feed_string(format!(
                                        "\r\n[crabdash] {error}\r\n"
                                    ));
                                }
                            }
                            cx.notify();
                            quake.controller.is_some()
                        })
                        .unwrap_or(false);

                    if !should_continue {
                        break;
                    }
                }
            }
        })
        .detach();
    }

    pub(crate) fn close_quake_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quake_terminal_open = false;
        self.cancel_quake_resize();
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(super) fn select_terminal_session(
        &mut self,
        machine: uuid::Uuid,
        session: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_machine().uuid != machine || !self.quake_terminal_open {
            return;
        }
        if let Some(drawer) = self.quake_terminals.get_mut(&machine) {
            drawer.model.layout.select(session);
            if let Some(session) = drawer.active() {
                window.focus(&session.input.focus_handle(cx));
            }
        }
        self.resize_quake_terminal(window, cx);
        cx.notify();
    }

    pub(super) fn close_terminal_session(
        &mut self,
        machine: uuid::Uuid,
        session: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_machine().uuid != machine {
            return;
        }
        let Some(drawer) = self.quake_terminals.get_mut(&machine) else {
            return;
        };
        if !drawer.sessions.contains_key(&session) {
            return;
        }
        if drawer.sessions.len() == 1 {
            if let Some(drawer) = self.quake_terminals.remove(&machine) {
                drawer.shutdown();
            }
            self.set_quake_terminal_open(false, window, cx);
            return;
        }
        if !drawer.model.remove(session) {
            return;
        }
        if let Some(session) = drawer.sessions.remove(&session)
            && let Some(controller) = session.controller
            && let Err(error) = controller.shutdown()
        {
            tracing::debug!(%error, "Failed to close terminal session");
        }
        if let Some(session) = drawer.active() {
            window.focus(&session.input.focus_handle(cx));
        }
        self.resize_quake_terminal(window, cx);
        cx.notify();
    }

    pub(super) fn drop_terminal_tab(
        &mut self,
        machine: uuid::Uuid,
        session: uuid::Uuid,
        pane: u32,
        drop: crate::layout::Drop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_machine().uuid != machine || !self.quake_terminal_open {
            return;
        }
        let Some(drawer) = self.quake_terminals.get_mut(&machine) else {
            return;
        };
        if drawer.model.drop_tab(session, pane, drop) {
            if let Some(session) = drawer.active() {
                window.focus(&session.input.focus_handle(cx));
            }
            self.resize_quake_terminal(window, cx);
            cx.notify();
        }
    }
}
