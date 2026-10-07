use super::mutation::{self, Operation};
use crate::app::Crabdash;
use gpui::*;

impl Crabdash {
    pub(crate) fn toggle_login_startup(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = mutation::begin(cx, Operation::LoginStartup) {
            self.preference_editor.mutation_error = Some(error.into());
            cx.notify();
            return;
        }
        let enabled = match crate::desktop::startup::begin_toggle(cx) {
            Ok(enabled) => enabled,
            Err(error) => {
                mutation::complete(cx, Operation::LoginStartup);
                self.preference_editor.mutation_error = Some(error.into());
                cx.notify();
                return;
            }
        };
        // A previous busy message belongs to its completed operation. Keep
        // unrelated validation/save errors while starting native registration.
        self.preference_editor.mutation_error = None;
        self.synchronize_login_startup(cx);
        cx.spawn(async move |_: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let result = cx
                .background_spawn(async move {
                    let result = crate::desktop::startup::set_login_startup(enabled);
                    (result, crate::desktop::startup::LoginStartup::load())
                })
                .await;
            // Native state and both guards belong to the application. Publish
            // and release them even if the initiating dashboard has closed.
            let _ = cx.update(|cx| {
                let (result, startup) = result;
                crate::desktop::startup::complete_toggle(
                    cx,
                    startup,
                    result.err().map(|error| error.to_string()),
                );
                mutation::complete(cx, Operation::LoginStartup);
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn apply_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preference_editor.busy {
            return;
        }
        let settings = match self.preference_editor.collect(cx) {
            Ok(settings) => settings,
            Err(error) => {
                self.preference_editor.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        if let Err(error) = mutation::begin(cx, Operation::Preferences) {
            self.preference_editor.mutation_error = Some(error.into());
            cx.notify();
            return;
        }
        window.focus(&self.focus_handle);
        self.preference_editor.busy = true;
        self.preference_editor.error = None;
        self.preference_editor.mutation_error = None;
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let saved = settings.clone();
            let result = cx.background_spawn(async move { saved.save() }).await;
            // The initiating window can close during disk I/O. Release the
            // shared guard and update other windows independently of its owner.
            let _ = cx.update(|cx| {
                if result.is_ok() {
                    cx.set_global(settings.clone());
                    crate::desktop::startup::set_start_minimised(cx, settings.start_minimised);
                }
                mutation::complete(cx, Operation::Preferences);
            });
            this.update(cx, |this, cx| {
                this.preference_editor.busy = false;
                match result {
                    Err(error) => this.preference_editor.error = Some(error.to_string()),
                    Ok(()) => {
                        this.update_saved_preferences(settings, cx);
                        this.preferences_open = false;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn synchronize_login_startup(&mut self, cx: &mut Context<Self>) {
        let runtime = cx.global::<crate::desktop::startup::Runtime>();
        self.login_startup = runtime.status();
        self.startup_busy = runtime.startup_busy();
    }

    pub(crate) fn update_saved_preferences(
        &mut self,
        settings: crate::features::preferences::Preferences,
        cx: &mut Context<Self>,
    ) {
        let sidebar_changed = self.preferences.sidebar_width != settings.sidebar_width;
        if sidebar_changed {
            self.sync_workspace_store(cx);
        }
        if self.preferences.sidebar_width != settings.sidebar_width {
            self.sidebar_width = px(settings.sidebar_width);
        }
        if self.preferences.terminal_rows != settings.terminal_rows {
            self.quake_height = px(crate::components::style::BAR * settings.interface_font_size
                / crate::components::style::TEXT
                + 26.0
                + f32::from(settings.terminal_rows)
                    * (settings.terminal_font_size * settings.terminal_line_height).ceil());
        }
        if self.preferences.log_lines != settings.log_lines {
            self.expanded_docker_logs.clear();
            self.logs_open_containers.clear();
            self.expanded_service_logs.clear();
            self.logs_open_services.clear();
        }
        self.login_startup.start_minimised = settings.start_minimised;
        self.preferences = settings;
        if sidebar_changed {
            self.persist_workspace(cx);
        }
    }
}
