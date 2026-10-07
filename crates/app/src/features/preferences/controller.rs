use crate::app::Crabdash;
use gpui::*;

impl Crabdash {
    pub(crate) fn toggle_login_startup(&mut self, cx: &mut Context<Self>) {
        if self.startup_busy {
            return;
        }
        self.startup_busy = true;
        self.login_startup.error = None;
        let enabled = !self.login_startup.enabled;
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let result = cx
                .background_spawn(
                    async move { crate::desktop::startup::set_login_startup(enabled) },
                )
                .await;
            this.update(cx, |this, cx| {
                this.startup_busy = false;
                match result {
                    Ok(()) => this.login_startup.enabled = enabled,
                    Err(error) => this.login_startup.error = Some(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn apply_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preference_editor.busy || self.startup_busy {
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
        window.focus(&self.focus_handle);
        self.preference_editor.busy = true;
        self.preference_editor.error = None;
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let saved = settings.clone();
            let result = cx.background_spawn(async move { saved.save() }).await;
            this.update(cx, |this, cx| {
                this.preference_editor.busy = false;
                match result {
                    Err(error) => this.preference_editor.error = Some(error.to_string()),
                    Ok(()) => {
                        cx.set_global(settings.clone());
                        this.update_saved_preferences(settings);
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

    pub(crate) fn update_saved_preferences(
        &mut self,
        settings: crate::features::preferences::Preferences,
    ) {
        if self.preferences.sidebar_width != settings.sidebar_width {
            self.sidebar_width = px(settings.sidebar_width);
        }
        if self.preferences.terminal_rows != settings.terminal_rows {
            self.quake_height = px(36.0 * settings.interface_font_size / 13.0
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
    }
}
