use crate::features::machines::AddMachineAuthMode;
use std::collections::{HashMap, HashSet};

use crate::components::{common::LucideIcon, text_field::TextField};
use crate::features::{
    docker::DockerRunConfig,
    machines::{add_modal as modal, sidebar},
    notifications as toast, preferences,
};
use crate::{
    AboutCrabdash, CloseWindow, DismissAddMachineModal, MinimizeWindow, OpenAddMachine,
    OpenPreferences, RefreshServices, ToggleAppMenu, ToggleFullScreen, ToggleSidebar,
    ToggleTerminal, ZoomWindow, content, features, show_about_dialog,
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
}

impl MainTab {
    pub(crate) fn shortcut(self) -> &'static str {
        match self {
            Self::Docker => crate::desktop::menus::shortcut("⌘1", "Ctrl+1"),
            Self::Disks => crate::desktop::menus::shortcut("⌘2", "Ctrl+2"),
            Self::Services => crate::desktop::menus::shortcut("⌘3", "Ctrl+3"),
        }
    }
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
    pub(crate) workspaces: features::workspaces::State,
    pub(crate) detached_windows:
        HashMap<(Uuid, u32), WindowHandle<crate::desktop::window::DetachedWorkspace>>,
    pub(crate) detached_reconcile_scheduled: bool,
    pub(crate) docker_refresh_generation: HashMap<Uuid, u64>,
    pub(crate) docker_table: features::docker::table::State,
    pub(crate) disks_table: features::disks::table::State,
    pub(crate) services_table: features::services::table::State,
    pub(crate) pending_docker_actions: HashMap<(Uuid, String), DockerAction>,
    pub(crate) pending_service_actions: HashMap<String, ServiceAction>,
    pub(crate) expanded_disk_rows: HashSet<String>,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) sidebar_width: Pixels,
    pub(crate) status_message: Option<String>,
    pub(crate) add_machine_modal_open: bool,
    pub(crate) preferences_open: bool,
    pub(crate) preferences: crate::features::preferences::Preferences,
    pub(crate) preference_editor: preferences::Editor,
    _preference_changes: Subscription,
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
    pub(crate) services_scroll_handle: ScrollHandle,
    pub(crate) quake_terminals: HashMap<Uuid, features::terminal::QuakeTerminal>,
    pub(crate) quake_terminal_open: bool,
    pub(crate) terminal_window: Option<WindowId>,
    pub(crate) overlay_window: Option<WindowId>,
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
        self.sync_state(cx);
        self.refresh_docker(cx);
        self.refresh_disks(cx);
        self.refresh_system_services(cx);
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
                this.update_saved_preferences(crate::features::preferences::current(cx));
                cx.notify();
            });
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
            detached_windows: HashMap::new(),
            detached_reconcile_scheduled: false,
            docker_refresh_generation: HashMap::new(),
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
            login_startup: crate::desktop::startup::LoginStartup::load(),
            startup_busy: false,
            open_menu: None,
            menu_item: 0,
            _menu_keystrokes: menu_keystrokes,
            expanded_docker_logs: HashMap::default(),
            logs_open_containers: HashSet::default(),
            expanded_service_logs: HashMap::default(),
            logs_open_services: HashSet::default(),
            docker_scroll_handle: ScrollHandle::new(),
            disks_scroll_handle: ScrollHandle::new(),
            services_scroll_handle: ScrollHandle::new(),
            quake_terminals: HashMap::default(),
            quake_terminal_open: false,
            terminal_window: None,
            overlay_window: None,
            quake_height: px(36.0
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
        app.refresh_services(cx);
        app.start_update_loop(cx);
        app
    }

    pub(crate) fn dismiss_add_machine_modal_action(
        &mut self,
        _: &DismissAddMachineModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.docker_removal.take().is_some() {
            self.focus_handle.focus(window);
            cx.notify();
        } else if self.add_machine_modal_open {
            self.close_add_machine_modal(window, cx);
        }
    }
}

impl Crabdash {
    pub(crate) fn render_workspace(
        &mut self,
        detached: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let title = if detached.is_some() {
            format!("Crabdash — {}", self.workspaces.store.current().name)
        } else {
            "Crabdash".into()
        };
        window.set_window_title(&title);
        if detached.is_none() {
            if self.quake_terminal_open
                && !cx
                    .windows()
                    .iter()
                    .any(|handle| Some(handle.window_id()) == self.terminal_window)
            {
                self.terminal_window = Some(window.window_handle().window_id());
            }
            self.apply_workspace_runtime(window, cx);
        }
        crate::desktop::window::schedule_workspace_windows(self, cx);
        window.set_rem_size(px(16.0 * self.preferences.interface_font_size / 13.0));
        self.resize_quake_terminal(window, cx);

        let overlay_here = self
            .overlay_window
            .is_none_or(|id| id == window.window_handle().window_id());
        let root = div()
            .font_family(if self.preferences.interface_font.is_empty() {
                SharedString::from(".SystemUIFont")
            } else {
                self.preferences.interface_font.clone().into()
            })
            .text_size(gpui::rems(crate::components::style::TEXT / 16.0))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &crate::ShowDocker, _, cx| {
                this.select_workspace_tab(MainTab::Docker, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::ShowDisks, _, cx| {
                this.select_workspace_tab(MainTab::Disks, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::ShowServices, _, cx| {
                this.select_workspace_tab(MainTab::Services, cx);
            }))
            .on_action(cx.listener(|_, _: &AboutCrabdash, window, cx| {
                show_about_dialog(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenPreferences, window, cx| {
                this.preference_editor = preferences::Editor::new(&this.preferences, cx);
                this.preferences_open = true;
                this.overlay_window = Some(window.window_handle().window_id());
                this.login_startup = crate::desktop::startup::LoginStartup::load();
                window.focus(&this.focus_handle);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleAppMenu, window, cx| {
                this.open_menu = if this.open_menu.is_some() {
                    None
                } else {
                    Some(0)
                };
                this.overlay_window = Some(window.window_handle().window_id());
                this.menu_item = 0;
                window.focus(&this.focus_handle);
                cx.notify();
            }))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.preferences_open
                    && this.overlay_window == Some(window.window_handle().window_id())
                    && !this.preference_editor.busy
                    && event.keystroke.key == "escape"
                {
                    this.preferences_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }
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
                            .when(detached.is_none() && !self.sidebar_collapsed, |this| {
                                this.on_drag_move(cx.listener(
                                    |this,
                                     event: &DragMoveEvent<sidebar::DraggedSidebarResize>,
                                     _window,
                                     cx| {
                                        this.set_sidebar_width(event.event.position.x, cx);
                                    },
                                ))
                            })
                            .when(detached.is_none() && !self.sidebar_collapsed, |this| {
                                this.child(sidebar::render(self, cx))
                            })
                            .child(match detached {
                                Some(id) => content::render_detached(self, id, window, cx),
                                None => content::render(self, window, cx),
                            }),
                    ),
            )
            .child(crate::desktop::window::resize_handles(window))
            .when(self.terminal_is_open_in(window), |this| {
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
            .when(overlay_here && self.preferences_open, |this| {
                this.child(preferences::render(self, window, cx))
            })
            .when(overlay_here && self.workspaces.open, |this| {
                this.child(features::workspaces::popup(self, cx))
            })
            .when(overlay_here && self.open_menu.is_some(), |this| {
                this.child(crate::desktop::menus::popup(self, window, cx))
            })
            .when(overlay_here && self.add_machine_modal_open, |this| {
                this.child(modal::render(self, cx))
            })
            .when(overlay_here && self.docker_run_modal_open, |this| {
                this.child(features::docker::run_modal::render(self, window, cx))
            })
            .when(overlay_here && self.docker_removal.is_some(), |this| {
                this.child(features::docker::remove_modal::render(self, cx))
            });
        crate::desktop::appearance::frame(root, window)
    }
}

impl Render for Crabdash {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_workspace(None, window, cx)
    }
}
