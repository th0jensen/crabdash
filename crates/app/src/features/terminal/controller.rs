use crate::components::terminal_input::{TerminalInput, TerminalInputEvent};
use crate::{app::Crabdash, features::terminal};
use gpui::*;
use machines::terminal::{TerminalEvent, TerminalSize};

impl Crabdash {
    pub(crate) fn active_quake_terminal(&self) -> Option<&terminal::QuakeTerminal> {
        self.quake_terminals.get(&self.selected_machine().uuid)
    }

    fn active_quake_terminal_mut(&mut self) -> Option<&mut terminal::QuakeTerminal> {
        let machine_uuid = self.selected_machine().uuid;
        self.quake_terminals.get_mut(&machine_uuid)
    }

    pub(crate) fn toggle_quake_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        if self.quake_terminal_open {
            self.close_quake_terminal(window, cx);
        } else {
            self.open_quake_terminal(window, cx);
        }
    }

    fn quake_terminal_size(&self, window: &Window, cx: &App) -> TerminalSize {
        let (cell_width, cell_height) = terminal::cell_metrics(&self.preferences, cx);
        let available_width = (window.viewport_size().width - px(20.0)).max(px(80.0));
        let columns = (available_width / px(cell_width))
            .floor()
            .clamp(20.0, u16::MAX as f32) as u16;
        let rows = terminal::quake_rows_for_height(
            self.quake_height,
            cell_height,
            f32::from(window.rem_size()) * 36.0 / 16.0,
        );
        TerminalSize {
            columns,
            rows,
            pixel_width: columns.saturating_mul(cell_width.ceil() as u16),
            pixel_height: rows.saturating_mul(cell_height as u16),
        }
    }

    /// Snap the quake panel height to whole terminal rows and apply it,
    /// resizing the Ghostty terminal and PTY when the row count changed.
    pub(crate) fn set_quake_height(
        &mut self,
        height: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let (_, cell_height) = terminal::cell_metrics(&self.preferences, cx);
        let header_height = f32::from(window.rem_size()) * 36.0 / 16.0;
        let snapped = terminal::quake_height_for_rows(
            terminal::quake_rows_for_height(height, cell_height, header_height),
            cell_height,
            header_height,
        );
        if self.quake_height == snapped {
            return;
        }
        self.quake_height = snapped;
        self.resize_quake_terminal(window, cx);
        cx.notify();
    }

    pub(crate) fn resize_quake_terminal(&mut self, window: &Window, cx: &App) {
        if !self.quake_terminal_open {
            return;
        }

        let (cell_width, cell_height) = terminal::cell_metrics(&self.preferences, cx);
        let size = self.quake_terminal_size(window, cx);
        let Some(quake) = self
            .active_quake_terminal_mut()
            .filter(|quake| quake.size != size)
        else {
            return;
        };

        if let Err(error) = quake.terminal.resize(
            size.columns,
            size.rows,
            cell_width.ceil() as u32,
            cell_height as u32,
        ) {
            quake.status = terminal::QuakeTerminalStatus::Failed;
            tracing::warn!(%error, "Failed to resize Ghostty terminal");
            return;
        }
        if let Some(controller) = quake.controller.as_ref()
            && let Err(error) = controller.resize(size)
        {
            quake.status = terminal::QuakeTerminalStatus::Failed;
            tracing::warn!(%error, "Failed to resize terminal PTY");
            return;
        }
        quake.size = size;
    }

    pub(crate) fn open_quake_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.activate_window();
        self.quake_terminal_open = true;
        let machine = self.selected_machine().clone();
        let machine_uuid = machine.uuid;

        if let Some(quake) = self.quake_terminals.get(&machine_uuid)
            && matches!(
                quake.status,
                terminal::QuakeTerminalStatus::Connected
                    | terminal::QuakeTerminalStatus::Connecting
            )
        {
            window.focus(&quake.input.focus_handle(cx));
            self.resize_quake_terminal(window, cx);
            cx.notify();
            return;
        }

        if let Some(previous) = self.quake_terminals.remove(&machine_uuid)
            && let Some(controller) = previous.controller
        {
            controller.shutdown().ok();
        }

        let endpoint = machine
            .remote
            .as_ref()
            .map(|remote| format!("{}@{}", remote.user, remote.host))
            .unwrap_or_else(|| "Local shell".to_string());
        let size = self.quake_terminal_size(window, cx);
        let terminal = match terminal::TerminalState::new_interactive(
            size.columns,
            size.rows,
            self.preferences.scrollback_lines,
        ) {
            Ok(terminal) => terminal,
            Err(error) => {
                self.set_status_error(format!("Failed to initialise Ghostty terminal: {error}"));
                cx.notify();
                return;
            }
        };
        let input = cx.new(TerminalInput::new);
        let input_events = cx.subscribe(&input, move |this, _, event: &TerminalInputEvent, cx| {
            let Some(quake) = this.quake_terminals.get_mut(&machine_uuid) else {
                return;
            };
            let data = match event {
                TerminalInputEvent::Input(bytes) => quake.terminal.prepare_input(bytes),
                TerminalInputEvent::Paste(text) => quake.terminal.prepare_paste(text),
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
        self.quake_terminals.insert(
            machine_uuid,
            terminal::QuakeTerminal {
                machine_name: machine.system_info.machine_name.clone(),
                endpoint,
                terminal,
                controller: None,
                size,
                status: terminal::QuakeTerminalStatus::Connecting,
                input: input.clone(),
                _input_events: input_events,
            },
        );
        window.focus(&input.focus_handle(cx));
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
                            if let Some(quake) = this.quake_terminals.get_mut(&machine_uuid) {
                                quake.status = terminal::QuakeTerminalStatus::Failed;
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
                        let Some(quake) = this.quake_terminals.get_mut(&machine_uuid) else {
                            return false;
                        };
                        quake.controller = Some(controller.clone());
                        quake.input.update(cx, |input, cx| {
                            input.set_controller(Some(controller.clone()), cx);
                        });
                        if quake.size != size
                            && let Err(error) = controller.resize(quake.size)
                        {
                            quake.status = terminal::QuakeTerminalStatus::Failed;
                            tracing::warn!(%error, "Failed to apply current terminal size");
                            return false;
                        }
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
                            let Some(quake) = this.quake_terminals.get_mut(&machine_uuid) else {
                                return false;
                            };

                            match event {
                                TerminalEvent::Output(output) => {
                                    quake.terminal.feed(output);
                                    for response in quake.terminal.take_pty_writes() {
                                        if let Err(error) = controller.write(response) {
                                            quake.status =
                                                terminal::QuakeTerminalStatus::Failed;
                                            tracing::warn!(%error, "Failed to send Ghostty PTY response");
                                        }
                                    }
                                }
                                TerminalEvent::Exited(status) => {
                                    quake.controller = None;
                                    quake.input.update(cx, |input, cx| {
                                        input.set_controller(None, cx);
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
                                        input.set_controller(None, cx);
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
        window.activate_window();
        self.quake_terminal_open = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }
}
