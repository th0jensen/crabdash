use anyhow::anyhow;
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::components::common::LucideIcon;
use crate::components::text_field::{
    FieldBackspace, FieldCopy, FieldCut, FieldDelete, FieldEnd, FieldHome, FieldLeft, FieldPaste,
    FieldRight, FieldSelectAll, FieldSelectLeft, FieldSelectRight, FieldTab, FieldTabPrev,
    TextField,
};
use crate::components::{content, modal, sidebar};
use machines::{machine::Machine, store::MachineStore};
use services::docker::Docker;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MainTab {
    #[default]
    Docker,
    Disks,
    Services,
}

actions!(crabdash, [CloseWindow]);

impl MainTab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Disks => "Disks",
            Self::Services => "Services",
        }
    }

    pub(crate) fn subtitle(self) -> &'static str {
        match self {
            Self::Docker => "Containers and workloads",
            Self::Disks => "Volumes and storage health",
            Self::Services => "Units and background processes",
        }
    }

    pub(crate) fn icon(self) -> LucideIcon {
        match self {
            Self::Docker => Icon::Boxes,
            Self::Disks => Icon::HardDrive,
            Self::Services => Icon::SquareTerminal,
        }
    }
}

pub struct Crabdash {
    pub(crate) machine_store: MachineStore,
    pub(crate) selected_machine: usize,
    pub(crate) active_tab: MainTab,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) status_message: Option<String>,
    pub(crate) add_machine_modal_open: bool,
    pub(crate) remote_host_field: Entity<TextField>,
    pub(crate) remote_user_field: Entity<TextField>,
    pub(crate) remote_password_field: Entity<TextField>,
    pub(crate) add_machine_error: Option<anyhow::Error>,
}

impl Crabdash {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (machine_store, status_message) = match MachineStore::load() {
            Ok(store) => (store, None),
            Err(error) => {
                eprintln!("Failed to load MachineStore {error}");
                (
                    MachineStore::default(),
                    Some(format!("Failed to load saved machines: {error}")),
                )
            }
        };

        let mut app = Self {
            machine_store,
            selected_machine: 0,
            active_tab: MainTab::default(),
            sidebar_collapsed: false,
            status_message,
            add_machine_modal_open: false,
            remote_host_field: cx.new(|cx| TextField::new("Host", "server.example.com", 1, cx)),
            remote_user_field: cx.new(|cx| TextField::new("User", "thomas", 2, cx)),
            remote_password_field: cx.new(|cx| TextField::new("Password", "password", 3, cx)),
            add_machine_error: None,
        };
        app.refresh_services();
        app
    }

    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseWindow, None),
            KeyBinding::new("backspace", FieldBackspace, None),
            KeyBinding::new("delete", FieldDelete, None),
            KeyBinding::new("left", FieldLeft, None),
            KeyBinding::new("right", FieldRight, None),
            KeyBinding::new("shift-left", FieldSelectLeft, None),
            KeyBinding::new("shift-right", FieldSelectRight, None),
            KeyBinding::new("cmd-a", FieldSelectAll, None),
            KeyBinding::new("cmd-v", FieldPaste, None),
            KeyBinding::new("cmd-c", FieldCopy, None),
            KeyBinding::new("cmd-x", FieldCut, None),
            KeyBinding::new("home", FieldHome, None),
            KeyBinding::new("end", FieldEnd, None),
            KeyBinding::new("tab", FieldTab, None),
            KeyBinding::new("shift-tab", FieldTabPrev, None),
        ]);
    }

    pub(crate) fn selected_machine(&self) -> &Machine {
        &self.machine_store.machines[self.selected_machine]
    }

    pub(crate) fn selected_machine_mut(&mut self) -> &mut Machine {
        &mut self.machine_store.machines[self.selected_machine]
    }

    pub(crate) fn refresh_services(&mut self) {
        match self.active_tab {
            MainTab::Docker => match self.selected_machine_mut().list_docker() {
                Ok(services) => {
                    let machine = self.selected_machine_mut();
                    machine.services.docker = services;
                    machine.services.docker_error = None;
                    self.clear_status_message();
                }
                Err(error) => {
                    let message = format!("Unable to load Docker: {error}");
                    let machine = self.selected_machine_mut();
                    machine.services.docker.clear();
                    machine.services.docker_error = Some(error);
                    self.set_status_error(message);
                }
            },
            MainTab::Disks => {}
            MainTab::Services => {}
        }
    }

    pub(crate) fn open_add_machine_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_machine_modal_open = true;
        self.add_machine_error = None;
        window.focus(&self.remote_host_field.focus_handle(cx));
        cx.notify();
    }

    pub(crate) fn close_add_machine_modal(&mut self, cx: &mut Context<Self>) {
        self.add_machine_modal_open = false;
        self.add_machine_error = None;
        cx.notify();
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        cx.notify();
    }

    pub(crate) fn set_status_error(&mut self, message: impl Into<String>) {
        let normalized = message
            .into()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        self.status_message = Some(normalized);
    }

    pub(crate) fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    fn clear_remote_machine_form(&mut self, cx: &mut Context<Self>) {
        self.remote_host_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_user_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_password_field
            .update(cx, |field, cx| field.clear(cx));
    }

    pub(crate) fn submit_add_machine(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.add_machine_error = None;

        let host = self.remote_host_field.read(cx).text().trim().to_string();
        let user = self.remote_user_field.read(cx).text().trim().to_string();
        let password = self.remote_password_field.read(cx).text();

        if host.is_empty() || user.is_empty() || password.trim().is_empty() {
            let error = anyhow!("Host, user, and password are required.");
            self.set_status_error(error.to_string());
            self.add_machine_error = Some(error);
            cx.notify();
            return;
        }

        match self.machine_store.add_remote_machine(user, host, password) {
            Ok(index) => {
                self.selected_machine = index;
                self.add_machine_modal_open = false;
                self.clear_remote_machine_form(cx);
                self.refresh_services();
                self.clear_status_message();
            }
            Err(error) => {
                self.set_status_error(error.to_string());
                self.add_machine_error = Some(error);
            }
        }

        cx.notify();
    }

    pub(crate) fn focus_next(
        &mut self,
        _: &FieldTab,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.add_machine_modal_open {
            window.focus_next();
        }
    }

    pub(crate) fn focus_prev(
        &mut self,
        _: &FieldTabPrev,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.add_machine_modal_open {
            window.focus_prev();
        }
    }
}

impl Render for Crabdash {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title("Crabdash");

        div()
            .track_focus(&self.focus_handle)
            .on_action(|_: &CloseWindow, window, _| {
                window.remove_window();
            })
            .relative()
            .size_full()
            .bg(rgb(0x18181A))
            .text_color(white())
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(content::render_title_bar(self, cx).on_mouse_down(
                        MouseButton::Left,
                        |_, window, _| {
                            window.start_window_move();
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .when(!self.sidebar_collapsed, |this| {
                                this.child(sidebar::render(self, cx))
                            })
                            .child(content::render(self, cx)),
                    )
                    .child(
                        div()
                            .h(px(36.0))
                            .px(px(20.0))
                            .border_t_1()
                            .border_color(rgb(0x3A3A3C))
                            .flex()
                            .items_center(),
                    ),
            )
            .when_some(self.status_message.as_ref(), |this, message| {
                this.child(
                    div()
                        .absolute()
                        .right(px(20.0))
                        .bottom(px(56.0))
                        .child(toast::render(message.clone(), cx)),
                )
            })
            .when(self.add_machine_modal_open, |this| {
                this.child(modal::render(self, cx))
            })
    }
}
