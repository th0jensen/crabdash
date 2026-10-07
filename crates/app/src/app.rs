use crate::features::machines::AddMachineAuthMode;
use std::collections::{HashMap, HashSet};

use crate::components::{common::LucideIcon, text_field::TextField};
use crate::features::{
    docker::DockerRunConfig,
    machines::{add_modal as modal, sidebar},
    notifications as toast, preferences,
};
use crate::{
    AboutCrabdash, CloseWindow, DismissModal, MinimizeWindow, OpenAddMachine, OpenPreferences,
    RefreshServices, ToggleAppMenu, ToggleFullScreen, ToggleSidebar, ToggleTerminal, ZoomWindow,
    content, features, show_about_dialog,
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use machines::store::{MachineStore, load_store};
use services::{ServiceAction, docker::DockerAction};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MainTab {
    #[default]
    Docker,
    Disks,
    Services,
    System,
}

impl MainTab {
    pub(crate) fn shortcut(self) -> &'static str {
        match self {
            Self::Docker => crate::desktop::menus::shortcut("⌘1", "Ctrl+1"),
            Self::Disks => crate::desktop::menus::shortcut("⌘2", "Ctrl+2"),
            Self::Services => crate::desktop::menus::shortcut("⌘3", "Ctrl+3"),
            Self::System => crate::desktop::menus::shortcut("⌘4", "Ctrl+4"),
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Disks => "Disks",
            Self::Services => "Services",
            Self::System => "System",
        }
    }

    pub(crate) fn icon(self) -> LucideIcon {
        match self {
            Self::Docker => Icon::Boxes,
            Self::Disks => Icon::HardDrive,
            Self::Services => Icon::SquareTerminal,
            Self::System => Icon::Cpu,
        }
    }
}

pub struct Crabdash {
    pub(crate) machine_store: MachineStore,
    pub(crate) selected_machine: usize,
    pub(crate) active_tab: MainTab,
    pub(crate) workspaces: features::workspaces::State,
    pub(crate) system: features::system::State,
    pub(crate) docker_refresh: features::refresh::Requests,
    pub(crate) docker_details: features::docker::details::State,
    pub(crate) disks_refresh: features::refresh::Requests,
    pub(crate) services_refresh: features::refresh::Requests,
    pub(crate) machine_refresh: features::refresh::Requests,
    pub(crate) polling: features::polling::State,
    pub(crate) dashboard_visibility: crate::desktop::window::visibility::State,
    pub(crate) machine_selection_generation: u64,
    pub(crate) docker_log_refresh: features::refresh::Requests<(Uuid, String)>,
    pub(crate) service_log_refresh: features::refresh::Requests<(Uuid, String)>,
    pub(crate) docker_action_requests: features::refresh::Requests<(Uuid, String)>,
    pub(crate) service_action_requests: features::refresh::Requests<(Uuid, String)>,
    pub(crate) docker_table: features::docker::table::State,
    pub(crate) disks_table: features::disks::table::State,
    pub(crate) services_table: features::services::table::State,
    pub(crate) pending_docker_actions: HashMap<(Uuid, String), DockerAction>,
    pub(crate) pending_service_actions: HashMap<(Uuid, String), ServiceAction>,
    pub(crate) expanded_disk_rows: HashSet<String>,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) sidebar_width: Pixels,
    pub(crate) status_message: Option<String>,
    pub(crate) add_machine_modal_open: bool,
    pub(crate) preferences_open: bool,
    pub(crate) preferences: crate::features::preferences::Preferences,
    pub(crate) preference_editor: preferences::Editor,
    _preference_changes: Subscription,
    _preference_mutations: Subscription,
    _startup_changes: Subscription,
    pub(crate) login_startup: crate::desktop::startup::LoginStartup,
    pub(crate) startup_busy: bool,
    pub(crate) open_menu: Option<usize>,
    pub(crate) menu_item: usize,
    _menu_keystrokes: Option<Subscription>,
    pub(crate) expanded_docker_logs: HashMap<(Uuid, String), features::terminal::TerminalState>,
    pub(crate) logs_open_containers: HashSet<(Uuid, String)>,
    pub(crate) expanded_service_logs: HashMap<(Uuid, String), features::terminal::TerminalState>,
    pub(crate) logs_open_services: HashSet<(Uuid, String)>,
    pub(crate) docker_scroll_handle: ScrollHandle,
    pub(crate) disks_scroll_handle: ScrollHandle,
    pub(crate) quake_terminals: HashMap<Uuid, features::terminal::QuakeTerminal>,
    pub(crate) quake_terminal_open: bool,
    pub(crate) quake_height: Pixels,
    pub(crate) docker_run_config: DockerRunConfig,
    pub(crate) docker_run_modal_open: bool,
    pub(crate) docker_removal: Option<features::docker::DockerRemoval>,
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

impl Crabdash {
    pub(crate) fn refresh_services(&mut self, cx: &mut Context<Self>) {
        self.refresh_machine(self.selected_machine().uuid, cx);
    }

    pub(crate) fn refresh_machine(&mut self, uuid: Uuid, cx: &mut Context<Self>) {
        self.sync_state_for(uuid, cx);
        self.refresh_docker_for(uuid, false, cx);
        self.refresh_disks_for(uuid, cx);
        self.refresh_system_services_for(uuid, false, cx);
        self.refresh_visible_system_resources_for(uuid, cx);
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let (machine_store, status_message) = smol::block_on(async {
            match load_store().await {
                Ok(store) => (store, None),
                Err(error) => {
                    tracing::error!(%error, "Failed to load MachineStore");
                    (
                        MachineStore::default(),
                        Some(format!("Failed to load saved machines: {error}")),
                    )
                }
            }
        });

        let menu_keystrokes = crate::desktop::menus::intercept(cx);

        let (settings, settings_error) = match crate::features::preferences::Preferences::load() {
            Ok(settings) => (settings, None),
            Err(error) => (
                crate::features::preferences::Preferences::default(),
                Some(error.to_string()),
            ),
        };
        cx.set_global(settings.clone());
        let preference_changes =
            cx.observe_global::<crate::features::preferences::Preferences>(|this, cx| {
                this.update_saved_preferences(crate::features::preferences::current(cx), cx);
                cx.notify();
            });
        crate::desktop::startup::initialize(cx);
        preferences::mutation::initialize(cx);
        let startup_changes = cx.observe_global::<crate::desktop::startup::Runtime>(|this, cx| {
            this.synchronize_login_startup(cx);
            cx.notify();
        });
        let preference_mutations = cx.observe_global::<preferences::mutation::State>(|this, cx| {
            if !preferences::mutation::is_busy(cx) {
                this.preference_editor.mutation_error = None;
            }
            cx.notify();
        });
        let startup_runtime = cx.global::<crate::desktop::startup::Runtime>();
        let login_startup = startup_runtime.status();
        let startup_busy = startup_runtime.startup_busy();
        let mut preference_editor = preferences::Editor::new(&settings, cx);
        preference_editor.error = settings_error;
        let workspaces = features::workspaces::State::load(cx);
        let active_tab = workspaces.layout().active().into();
        let sidebar_collapsed = workspaces.store.current().sidebar_collapsed;
        let sidebar_width = px(workspaces.store.current().sidebar_width);
        let mut app = Self {
            machine_store,
            selected_machine: 0,
            active_tab,
            workspaces,
            system: features::system::State::new(cx),
            docker_refresh: Default::default(),
            docker_details: Default::default(),
            disks_refresh: Default::default(),
            services_refresh: Default::default(),
            machine_refresh: Default::default(),
            polling: Default::default(),
            dashboard_visibility: Default::default(),
            machine_selection_generation: 0,
            docker_log_refresh: Default::default(),
            service_log_refresh: Default::default(),
            docker_action_requests: Default::default(),
            service_action_requests: Default::default(),
            docker_table: features::docker::table::State::new(cx),
            disks_table: features::disks::table::State::new(cx),
            services_table: features::services::table::State::new(cx),
            pending_docker_actions: HashMap::default(),
            pending_service_actions: HashMap::default(),
            expanded_disk_rows: HashSet::default(),
            sidebar_collapsed,
            sidebar_width,
            status_message,
            add_machine_modal_open: false,
            preferences_open: false,
            preferences: settings.clone(),
            preference_editor,
            _preference_changes: preference_changes,
            _preference_mutations: preference_mutations,
            _startup_changes: startup_changes,
            login_startup,
            startup_busy,
            open_menu: None,
            menu_item: 0,
            _menu_keystrokes: menu_keystrokes,
            expanded_docker_logs: HashMap::default(),
            logs_open_containers: HashSet::default(),
            expanded_service_logs: HashMap::default(),
            logs_open_services: HashSet::default(),
            docker_scroll_handle: ScrollHandle::new(),
            disks_scroll_handle: ScrollHandle::new(),
            quake_terminals: HashMap::default(),
            quake_terminal_open: false,
            quake_height: px(crate::components::style::BAR * settings.interface_font_size
                / crate::components::style::TEXT
                + 26.0
                + f32::from(settings.terminal_rows)
                    * (settings.terminal_font_size * settings.terminal_line_height).ceil()),
            docker_run_config: DockerRunConfig::new(cx),
            docker_run_modal_open: false,
            docker_removal: None,
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
        app.start_update_loop(cx);
        cx.on_release(|app, _| {
            for terminal in app.quake_terminals.values() {
                if let Some(controller) = &terminal.controller
                    && let Err(error) = controller.shutdown()
                {
                    tracing::debug!(%error, "Failed to shut down a released dashboard terminal");
                }
            }
        })
        .detach();
        app
    }

    pub(crate) fn submit_modal_action(
        &mut self,
        _: &crate::SubmitModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.add_machine_modal_open {
            self.submit_add_machine(window, cx);
        } else if self.workspaces.open
            && self.workspaces.rename.is_some()
            && self
                .workspaces
                .name
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
            && !self.preferences_open
            && !self.docker_run_modal_open
            && self.docker_removal.is_none()
        {
            self.finish_workspace_name(window, cx);
        }
    }

    pub(crate) fn dismiss_modal_action(
        &mut self,
        _: &DismissModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cx.stop_active_drag(window) {
            self.workspaces.drag_target = None;
            if self.workspaces.resizing_split.take().is_some() {
                // A divider changes the current ratio as it moves. Finish
                // that resize on Escape; tab drags have no pending mutation.
                self.persist_workspace(cx);
            } else {
                cx.notify();
            }
            return;
        }
        if self.docker_run_modal_open {
            if self.docker_run_config.busy {
                return;
            }
            self.docker_run_modal_open = false;
        } else if self.preferences_open {
            if self.preference_editor.busy {
                return;
            }
            self.preferences_open = false;
        } else if self.docker_removal.take().is_some() {
        } else if self.add_machine_modal_open {
            self.close_add_machine_modal(window, cx);
            return;
        } else if self.workspaces.open {
            if self.workspaces.rename.take().is_none() {
                self.workspaces.open = false;
            }
            self.workspaces.error = None;
            self.workspaces.rename_error = None;
        } else if self.open_menu.take().is_none() {
            return;
        }
        self.focus_handle.focus(window);
        cx.notify();
    }
}

impl Render for Crabdash {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title("Crabdash");
        window.set_rem_size(px(
            16.0 * self.preferences.interface_font_size / crate::components::style::TEXT
        ));
        self.apply_workspace_runtime(window, cx);
        self.prepare_visible_domains(cx);
        self.prepare_system_resources(cx);
        self.resize_quake_terminal(window, cx);

        let root = div()
            .font_family(if self.preferences.interface_font.is_empty() {
                SharedString::from(".SystemUIFont")
            } else {
                self.preferences.interface_font.clone().into()
            })
            .text_size(gpui::rems(crate::components::style::TEXT / 16.0))
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && cx.has_active_drag() {
                    this.dismiss_modal_action(&DismissModal, window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &crate::ShowDocker, _, cx| {
                this.select_workspace_tab(MainTab::Docker, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::ShowDisks, _, cx| {
                this.select_workspace_tab(MainTab::Disks, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::ShowServices, _, cx| {
                this.select_workspace_tab(MainTab::Services, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::ShowSystem, _, cx| {
                this.select_workspace_tab(MainTab::System, cx);
            }))
            .on_action(cx.listener(|_, _: &AboutCrabdash, window, cx| {
                show_about_dialog(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenPreferences, window, cx| {
                if !this.preference_editor.busy {
                    this.preference_editor = preferences::Editor::new(&this.preferences, cx);
                }
                this.preferences_open = true;
                if !preferences::mutation::is_busy(cx) {
                    crate::desktop::startup::refresh(cx);
                }
                this.synchronize_login_startup(cx);
                window.focus(&this.focus_handle);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleAppMenu, window, cx| {
                this.open_menu = if this.open_menu.is_some() {
                    None
                } else {
                    Some(0)
                };
                this.menu_item = 0;
                window.focus(&this.focus_handle);
                cx.notify();
            }))
            .on_modifiers_changed(cx.listener(|_, _: &ModifiersChangedEvent, _, cx| {
                cx.notify();
            }))
            .on_action(|_: &CloseWindow, window, cx| {
                crate::desktop::window::close_window(window, cx);
            })
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| {
                this.toggle_sidebar(cx);
            }))
            .on_action(cx.listener(|this, _: &RefreshServices, _window, cx| {
                this.refresh_services(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleTerminal, window, cx| {
                this.toggle_quake_terminal(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenAddMachine, window, cx| {
                this.open_add_machine_modal(window, cx);
            }))
            .on_action(cx.listener(Crabdash::dismiss_modal_action))
            .on_action(cx.listener(Crabdash::submit_modal_action))
            .on_action(|_: &MinimizeWindow, window, cx| {
                crate::desktop::window::minimize(window, cx);
            })
            .on_action(|_: &ZoomWindow, window, _| {
                crate::desktop::window::zoom(window);
            })
            .on_action(|_: &ToggleFullScreen, window, _| {
                window.toggle_fullscreen();
            })
            .relative()
            .size_full()
            .bg(crate::desktop::appearance::root_background())
            .text_color(white())
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(crate::desktop::window::render(self, window, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
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
                    ),
            )
            .child(crate::desktop::window::resize_handles(window))
            .when(self.quake_terminal_open, |this| {
                this.child(features::terminal::render_quake(self, window, cx))
            })
            .when_some(self.status_message.as_ref(), |this, message| {
                this.child(
                    div()
                        .absolute()
                        .right(px(20.0))
                        .bottom(px(56.0))
                        .child(toast::render(message.clone(), cx)),
                )
            })
            .when(self.preferences_open, |this| {
                this.child(preferences::render(self, window, cx))
            })
            .when(self.open_menu.is_some(), |this| {
                this.child(crate::desktop::menus::popup(self, window, cx))
            })
            .when(self.add_machine_modal_open, |this| {
                this.child(modal::render(self, cx))
            })
            .when(self.docker_run_modal_open, |this| {
                this.child(features::docker::run_modal::render(self, window, cx))
            })
            .when(self.docker_removal.is_some(), |this| {
                this.child(features::docker::remove_modal::render(self, cx))
            });
        crate::desktop::appearance::frame(root, window)
    }
}
