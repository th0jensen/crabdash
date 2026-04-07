use std::collections::{HashMap, HashSet};

use anyhow::anyhow;
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use services::Services;
use services::docker::{DockerAction, DockerFilter};

use crate::components::common::LucideIcon;
use crate::components::text_field::{
    FieldBackspace, FieldCopy, FieldCut, FieldDelete, FieldEnd, FieldHome, FieldLeft, FieldPaste,
    FieldRight, FieldSelectAll, FieldSelectLeft, FieldSelectRight, FieldTab, FieldTabPrev,
    TextField,
};
use crate::components::{modal, sidebar, toast};
use crate::content;
use crate::{
    AboutCrabdash, CloseWindow, DismissAddMachineModal, MinimizeWindow, OpenAddMachine,
    RefreshServices, SubmitAddMachineModal, ToggleFullScreen, ToggleSidebar, ZoomWindow,
    show_about_dialog,
};
use machines::{machine::Machine, remote_connection::AuthMethod, store::MachineStore};
use services::{disks::Disks, docker::Docker};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MainTab {
    #[default]
    Docker,
    Disks,
    Services,
}

impl MainTab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Disks => "Disks",
            Self::Services => "Services",
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
    pub(crate) docker_filter: DockerFilter,
    pub(crate) pending_docker_actions: HashMap<String, DockerAction>,
    pub(crate) expanded_disk_rows: HashSet<String>,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) sidebar_width: Pixels,
    pub(crate) status_message: Option<String>,
    pub(crate) add_machine_modal_open: bool,
    pub(crate) docker_scroll_handle: ScrollHandle,
    pub(crate) disks_scroll_handle: ScrollHandle,
    pub(crate) services_scroll_handle: ScrollHandle,
    pub(crate) remote_host_field: Entity<TextField>,
    pub(crate) remote_user_field: Entity<TextField>,
    pub(crate) add_machine_auth_mode: AddMachineAuthMode,
    pub(crate) remote_password_field: Entity<TextField>,
    pub(crate) remote_private_key_field: Entity<TextField>,
    pub(crate) remote_public_key_field: Entity<TextField>,
    pub(crate) remote_passphrase_field: Entity<TextField>,
    pub(crate) add_machine_error: Option<anyhow::Error>,
    pub focus_handle: FocusHandle,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AddMachineAuthMode {
    #[default]
    None,
    Password,
    AuthKey,
}

impl AddMachineAuthMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Password => "Password",
            Self::AuthKey => "Auth Key",
        }
    }
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
            docker_filter: DockerFilter::default(),
            pending_docker_actions: HashMap::default(),
            expanded_disk_rows: HashSet::default(),
            sidebar_collapsed: false,
            sidebar_width: px(sidebar::DEFAULT_SIDEBAR_WIDTH),
            status_message,
            add_machine_modal_open: false,
            docker_scroll_handle: ScrollHandle::new(),
            disks_scroll_handle: ScrollHandle::new(),
            services_scroll_handle: ScrollHandle::new(),
            remote_host_field: cx.new(|cx| TextField::new("Host", "server.example.com", 1, cx)),
            remote_user_field: cx.new(|cx| TextField::new("User", "user", 2, cx)),
            add_machine_auth_mode: AddMachineAuthMode::default(),
            remote_password_field: cx.new(|cx| TextField::new("Password", "password", 3, cx)),
            remote_private_key_field: cx
                .new(|cx| TextField::new("Private Key", "~/.ssh/id_ed25519", 4, cx)),
            remote_public_key_field: cx
                .new(|cx| TextField::new("Public Key (Optional)", "~/.ssh/id_ed25519.pub", 5, cx)),
            remote_passphrase_field: cx
                .new(|cx| TextField::new("Passphrase (Optional)", "passphrase", 6, cx)),
            add_machine_error: None,
            focus_handle: cx.focus_handle(),
        };
        app.refresh_services();
        app
    }

    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("cmd-s", ToggleSidebar, None),
            KeyBinding::new("cmd-n", OpenAddMachine, None),
            KeyBinding::new("cmd-r", RefreshServices, None),
            KeyBinding::new("cmd-w", CloseWindow, None),
            KeyBinding::new("cmd-m", MinimizeWindow, None),
            KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
            KeyBinding::new("escape", DismissAddMachineModal, None),
            KeyBinding::new("escape", DismissAddMachineModal, Some("CrabdashTextField")),
            KeyBinding::new("enter", SubmitAddMachineModal, None),
            KeyBinding::new("enter", SubmitAddMachineModal, Some("CrabdashTextField")),
            KeyBinding::new("return", SubmitAddMachineModal, None),
            KeyBinding::new("return", SubmitAddMachineModal, Some("CrabdashTextField")),
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
        if let Err(error) = self.selected_machine_mut().sync_system_info() {
            eprintln!("Failed to sync machine system info: {error}");
        }

        match self.active_tab {
            MainTab::Docker => match self.selected_machine_mut().list_docker() {
                Ok(containers) => {
                    let machine = self.selected_machine_mut();
                    machine.services.docker = containers;
                    machine.services.docker_error = None;
                    self.clear_status_message();
                }
                Err(error) => {
                    let message = format!("Unable to load Docker: {error}");
                    let machine = self.selected_machine_mut();
                    machine.services.docker.clear();
                    machine.services.docker_error = Some(error.to_string());
                    self.set_status_error(message);
                }
            },
            MainTab::Disks => match self.selected_machine_mut().list_disks() {
                Ok(services) => {
                    let machine = self.selected_machine_mut();
                    machine.services.disks = services;
                    machine.services.disks_error = None;
                    self.clear_status_message();
                }
                Err(error) => {
                    let message = format!("Unable to load Disks: {error}");
                    let machine = self.selected_machine_mut();
                    machine.services.disks.clear();
                    machine.services.disks_error = Some(error.to_string());
                    self.set_status_error(message);
                }
            },
            MainTab::Services => match self.selected_machine_mut().list_services() {
                Ok(services) => {
                    let machine = self.selected_machine_mut();
                    machine.services.systemd = services;
                    machine.services.systemd_error = None;
                    self.clear_status_message();
                }
                Err(error) => {
                    let message = format!("Unable to load Disks: {error}");
                    let machine = self.selected_machine_mut();
                    machine.services.systemd.clear();
                    machine.services.systemd_error = Some(error.to_string());
                    self.set_status_error(message);
                }
            },
        }
    }

    pub(crate) fn delete_machine(&mut self, uuid: Uuid, cx: &mut Context<Self>) {
        if let Err(error) = MachineStore::remove_machine(uuid) {
            let message = format!("Unable to delete machine: {error}");
            eprintln!("{message}");
            self.set_status_error(message);
            cx.notify();
            return;
        }

        match MachineStore::load() {
            Ok(store) => {
                self.machine_store = store;
                self.selected_machine = self
                    .selected_machine
                    .min(self.machine_store.machines.len().saturating_sub(1));
                self.refresh_services();
                self.clear_status_message();
            }
            Err(error) => {
                let message = format!("Unable to reload machines after delete: {error}");
                eprintln!("{message}");
                self.set_status_error(message);
            }
        }

        cx.notify();
    }

    pub(crate) fn open_add_machine_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_machine_modal_open = true;
        self.add_machine_error = None;
        window.focus(&self.remote_host_field.focus_handle(cx));
        cx.notify();
    }

    pub(crate) fn close_add_machine_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_machine_modal_open = false;
        self.add_machine_error = None;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(crate) fn dismiss_add_machine_modal_action(
        &mut self,
        _: &DismissAddMachineModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.add_machine_modal_open {
            self.close_add_machine_modal(window, cx);
        }
    }

    pub(crate) fn set_add_machine_auth_mode(
        &mut self,
        mode: AddMachineAuthMode,
        cx: &mut Context<Self>,
    ) {
        self.add_machine_auth_mode = mode;
        self.add_machine_error = None;
        cx.notify();
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        cx.notify();
    }

    pub(crate) fn toggle_disk_row(&mut self, disk_id: &str, cx: &mut Context<Self>) {
        if !self.expanded_disk_rows.insert(disk_id.to_string()) {
            self.expanded_disk_rows.remove(disk_id);
        }
        cx.notify();
    }

    pub(crate) fn set_sidebar_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        self.sidebar_width = sidebar::clamp_width(width);
        cx.notify();
    }

    pub(crate) fn set_status_error(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into().trim().to_string());
    }

    pub(crate) fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    fn clear_remote_machine_form(&mut self, cx: &mut Context<Self>) {
        self.remote_host_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_user_field
            .update(cx, |field, cx| field.clear(cx));
        self.add_machine_auth_mode = AddMachineAuthMode::Password;
        self.remote_password_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_private_key_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_public_key_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_passphrase_field
            .update(cx, |field, cx| field.clear(cx));
    }

    pub(crate) fn submit_add_machine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_machine_error = None;

        let host = self.remote_host_field.read(cx).text().trim().to_string();
        let user = self.remote_user_field.read(cx).text().trim().to_string();
        let auth = match self.add_machine_auth_mode {
            AddMachineAuthMode::None => Ok(AuthMethod::None),
            AddMachineAuthMode::Password => {
                if !self.remote_password_field.read(cx).text().is_empty() {
                    let password = self.remote_password_field.read(cx).text();
                    Ok(AuthMethod::Password(password))
                } else {
                    Err(anyhow!("Host, user, and password are required."))
                }
            }
            AddMachineAuthMode::AuthKey => {
                let private_key = self
                    .remote_private_key_field
                    .read(cx)
                    .text()
                    .trim()
                    .to_string();
                let public_key = self
                    .remote_public_key_field
                    .read(cx)
                    .text()
                    .trim()
                    .to_string();
                let passphrase = self
                    .remote_passphrase_field
                    .read(cx)
                    .text()
                    .trim()
                    .to_string();

                if private_key.is_empty() {
                    Err(anyhow!("Host, user, and private key are required."))
                } else {
                    Ok(AuthMethod::AuthKey {
                        pubkey: (!public_key.is_empty()).then(|| PathBuf::from(public_key)),
                        privatekey: PathBuf::from(private_key),
                        passphrase: (!passphrase.is_empty()).then_some(passphrase),
                    })
                }
            }
        };

        if host.is_empty() || user.is_empty() {
            let error = anyhow!("Host and user are required.");
            self.set_status_error(error.to_string());
            self.add_machine_error = Some(error);
            cx.notify();
            return;
        }

        let auth = match auth {
            Ok(auth) => auth,
            Err(error) => {
                self.set_status_error(error.to_string());
                self.add_machine_error = Some(error);
                cx.notify();
                return;
            }
        };

        match self.machine_store.add_remote_machine(user, host, auth) {
            Ok(index) => {
                self.selected_machine = index;
                self.add_machine_modal_open = false;
                self.clear_remote_machine_form(cx);
                self.refresh_services();
                self.clear_status_message();
                window.focus(&self.focus_handle);

                if let Some(rc) = self.selected_machine().remote.as_ref() {
                    let key = format!("com.thojensen.crabdash.ssh.{}@{}", rc.user, rc.host);
                    let user = rc.user.clone();
                    let auth = rc.auth.clone();
                    if let Some(secret) = auth.and_then(|auth| auth.secret_bytes()) {
                        cx.spawn(async move |_, cx| {
                            let future = cx
                                .update(|app| app.write_credentials(&key, &user, &secret))
                                .ok();
                            if let Some(future) = future {
                                future.await.ok();
                            }
                        })
                        .detach();
                    }
                }
            }
            Err(error) => {
                self.set_status_error(error.to_string());
                self.add_machine_error = Some(error);
            }
        }

        cx.notify();
    }

    pub(crate) fn submit_add_machine_action(
        &mut self,
        _: &SubmitAddMachineModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.add_machine_modal_open {
            self.submit_add_machine(window, cx);
        }
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
            .on_action(cx.listener(|_, _: &AboutCrabdash, _window, _cx| {
                show_about_dialog();
            }))
            .on_action(|_: &CloseWindow, window, _| {
                window.remove_window();
            })
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| {
                this.toggle_sidebar(cx);
            }))
            .on_action(cx.listener(|this, _: &RefreshServices, _window, cx| {
                this.refresh_services();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenAddMachine, window, cx| {
                this.open_add_machine_modal(window, cx);
            }))
            .on_action(cx.listener(Crabdash::dismiss_add_machine_modal_action))
            .on_action(cx.listener(Crabdash::submit_add_machine_action))
            .on_action(|_: &MinimizeWindow, window, _| {
                window.minimize_window();
            })
            .on_action(|_: &ZoomWindow, window, _| {
                window.zoom_window();
            })
            .on_action(|_: &ToggleFullScreen, window, _| {
                window.toggle_fullscreen();
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
                    .child(content::render_title_bar(self, window, cx).on_mouse_down(
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
                                this.on_drag_move(cx.listener(
                                    |this,
                                     event: &DragMoveEvent<sidebar::DraggedSidebarResize>,
                                     _window,
                                     cx| {
                                        this.set_sidebar_width(event.event.position.x, cx);
                                    },
                                ))
                            })
                            .when(!self.sidebar_collapsed, |this| {
                                this.child(sidebar::render(self, cx))
                            })
                            .child(content::render(self, cx)),
                    ), // .child(
                       //     div()
                       //         .h(px(36.0))
                       //         .px(px(20.0))
                       //         .border_t_1()
                       //         .border_color(rgb(0x2F2F31))
                       //         .flex()
                       //         .items_center(),
                       // ),
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
