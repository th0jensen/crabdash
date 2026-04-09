use std::collections::{HashMap, HashSet};

use anyhow::anyhow;
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use services::docker::{DockerAction, DockerFilter};
use services::{ServiceFilter, Services};

use crate::components::common::LucideIcon;
use crate::components::text_field::{
    FieldBackspace, FieldCopy, FieldCut, FieldDelete, FieldEnd, FieldHome, FieldLeft, FieldPaste,
    FieldRight, FieldSelectAll, FieldSelectLeft, FieldSelectRight, FieldTab, FieldTabPrev,
    TextField,
};
use crate::components::{modal, sidebar, toast};
use crate::content;
use crate::docker_run::DockerRunConfig;
use crate::{
    AboutCrabdash, CloseWindow, DismissAddMachineModal, DismissDockerLogModal, MinimizeWindow,
    OpenAddMachine, RefreshServices, SubmitAddMachineModal, ToggleFullScreen, ToggleSidebar,
    ZoomWindow, show_about_dialog,
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
    pub(crate) service_filter: ServiceFilter,
    pub(crate) pending_docker_actions: HashMap<String, DockerAction>,
    pub(crate) expanded_disk_rows: HashSet<String>,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) sidebar_width: Pixels,
    pub(crate) status_message: Option<String>,
    pub(crate) add_machine_modal_open: bool,
    pub(crate) expanded_docker_logs: HashMap<String, content::docker_logs::DockerLogState>,
    pub(crate) docker_log_modal: Option<String>,
    pub(crate) docker_scroll_handle: ScrollHandle,
    pub(crate) disks_scroll_handle: ScrollHandle,
    pub(crate) services_scroll_handle: ScrollHandle,
    pub(crate) docker_run_config: DockerRunConfig,
    pub(crate) docker_run_modal_open: bool,
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
            service_filter: ServiceFilter::default(),
            pending_docker_actions: HashMap::default(),
            expanded_disk_rows: HashSet::default(),
            sidebar_collapsed: false,
            sidebar_width: px(sidebar::DEFAULT_SIDEBAR_WIDTH),
            status_message,
            add_machine_modal_open: false,
            expanded_docker_logs: HashMap::default(),
            docker_log_modal: None,
            docker_scroll_handle: ScrollHandle::new(),
            disks_scroll_handle: ScrollHandle::new(),
            services_scroll_handle: ScrollHandle::new(),
            docker_run_config: DockerRunConfig::new(cx),
            docker_run_modal_open: false,
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
        app.refresh_services(cx);
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
            KeyBinding::new("escape", DismissDockerLogModal, None),
            KeyBinding::new("enter", DismissDockerLogModal, None),
            KeyBinding::new("return", DismissDockerLogModal, None),
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

    pub(crate) fn refresh_services(&mut self, cx: &mut Context<Self>) {
        let mut machine = self.selected_machine_mut().clone();
        let machine_index = self.selected_machine;
        let active_tab = self.active_tab;

        cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                if let Err(e) = machine.sync_system_info().await {
                    eprintln!("[app] sync_system_info failed: {e}");
                }

                let result: Result<(), anyhow::Error> = async {
                    match active_tab {
                        MainTab::Docker => {
                            let containers = machine.list_docker().await?;
                            this.update(&mut cx, move |this, cx| {
                                if let Some(m) = this.machine_store.machines.get_mut(machine_index)
                                {
                                    m.services.docker = containers;
                                    m.services.docker_error = None;
                                }
                                this.clear_status_message();
                                cx.notify();
                            })
                            .ok();
                        }
                        MainTab::Disks => {
                            let disks = machine.list_disks().await?;
                            this.update(&mut cx, move |this, cx| {
                                if let Some(m) = this.machine_store.machines.get_mut(machine_index)
                                {
                                    m.services.disks = disks;
                                    m.services.disks_error = None;
                                }
                                this.clear_status_message();
                                cx.notify();
                            })
                            .ok();
                        }
                        MainTab::Services => {
                            let services = machine.list_services().await?;
                            this.update(&mut cx, move |this, cx| {
                                if let Some(m) = this.machine_store.machines.get_mut(machine_index)
                                {
                                    m.services.systemd = services;
                                    m.services.systemd_error = None;
                                }
                                this.clear_status_message();
                                cx.notify();
                            })
                            .ok();
                        }
                    }
                    Ok(())
                }
                .await;

                if let Err(error) = result {
                    this.update(&mut cx, move |this, cx| {
                        let message = format!("Unable to load {active_tab:?}: {error}");
                        this.set_status_error(message);
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    pub(crate) fn open_docker_run_modal(&mut self, cx: &mut Context<Self>) {
        self.docker_run_modal_open = true;
        cx.notify();
    }

    pub(crate) fn close_docker_run_modal(&mut self, cx: &mut Context<Self>) {
        self.docker_run_modal_open = false;
        cx.notify();
    }

    pub(crate) fn submit_docker_run(&mut self, cx: &mut Context<Self>) {
        let image = self.docker_run_config.image.read(cx).text();
        if image.trim().is_empty() {
            self.set_status_error("Image name is required.");
            cx.notify();
            return;
        }

        let mut machine = self.selected_machine().clone();
        let machine_index = self.selected_machine;

        let args: Vec<String> = self.docker_run_config.build_args(cx);

        self.docker_run_modal_open = false;
        self.docker_run_config.reset(cx);
        cx.notify();

        cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = machine.run_container(args).await;

                match result {
                    Ok(_) => {
                        let containers = machine.list_docker().await;
                        this.update(&mut cx, move |this, cx| {
                            match containers {
                                Ok(containers) => {
                                    if let Some(m) =
                                        this.machine_store.machines.get_mut(machine_index)
                                    {
                                        m.services.docker = containers;
                                        m.services.docker_error = None;
                                    }
                                }
                                Err(err) => {
                                    eprintln!("Failed to refresh docker after run: {err}");
                                }
                            }
                            this.clear_status_message();
                            cx.notify();
                        })
                        .ok();
                    }
                    Err(err) => {
                        this.update(&mut cx, move |this, cx| {
                            this.set_status_error(format!("docker run failed: {err}"));
                            cx.notify();
                        })
                        .ok();
                    }
                }
            }
        })
        .detach();
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
                self.refresh_services(cx);
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

    pub(crate) fn submit_add_machine(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
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

        cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                match Machine::new_remote(&user, &host, auth).await {
                    Ok(machine) => {
                        this.update(&mut cx, move |this, cx| {
                            match this.machine_store.add_machine(machine) {
                                Ok(index) => {
                                    this.selected_machine = index;
                                    this.add_machine_modal_open = false;
                                    this.clear_remote_machine_form(cx);
                                    this.clear_status_message();
                                    this.refresh_services(cx);

                                    if let Some(rc) = this.selected_machine().remote.as_ref() {
                                        let key = format!(
                                            "com.thojensen.crabdash.ssh.{}@{}",
                                            rc.user, rc.host
                                        );
                                        let user = rc.user.clone();
                                        let auth = rc.auth.clone();
                                        if let Some(secret) =
                                            auth.and_then(|auth| auth.secret_bytes())
                                        {
                                            cx.spawn(async move |_, cx| {
                                                let future = cx
                                                    .update(|app| {
                                                        app.write_credentials(&key, &user, &secret)
                                                    })
                                                    .ok();
                                                if let Some(future) = future {
                                                    future.await.ok();
                                                }
                                            })
                                            .detach();
                                        }
                                    }
                                }
                                Err(err) => {
                                    this.set_status_error(err.to_string());
                                    this.add_machine_error = Some(err);
                                }
                            }
                            cx.notify();
                        })
                        .ok();
                    }
                    Err(error) => {
                        this.update(&mut cx, move |this, cx| {
                            this.set_status_error(error.to_string());
                            this.add_machine_error = Some(error);
                            cx.notify();
                        })
                        .ok();
                    }
                }
            }
        })
        .detach();
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
                this.refresh_services(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenAddMachine, window, cx| {
                this.open_add_machine_modal(window, cx);
            }))
            .on_action(cx.listener(Crabdash::dismiss_add_machine_modal_action))
            .on_action(cx.listener(|this, _: &DismissDockerLogModal, _, cx| {
                if this.docker_log_modal.is_some() {
                    let id = this.docker_log_modal.take();
                    if let Some(id) = id {
                        this.expanded_docker_logs.remove(&id);
                    }
                    cx.notify();
                }
            }))
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
                            .child(content::render(self, window, cx)),
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
            .when(self.docker_run_modal_open, |this| {
                this.child(content::render_docker_run_modal(self, cx))
            })
            .when(self.docker_log_modal.is_some(), |this| {
                this.child(content::render_logs_modal(self, cx))
            })
    }
}
