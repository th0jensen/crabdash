//! Per-dashboard backgrounding, independent of keyboard focus and SSH sessions.
use crate::{app::Crabdash, features::polling::Domains};
use gpui::*;

pub(crate) struct State {
    visible: bool,
    activation: Option<Subscription>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            visible: true,
            activation: None,
        }
    }
}

impl State {
    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    fn observe(&mut self, native: Option<bool>, active: bool) -> bool {
        // Wayland cannot report minimization. Preserve our explicit hide state
        // until activation, while an unfocused visible dashboard stays live.
        let visible = native.unwrap_or(self.visible || active);
        let changed = self.visible != visible;
        self.visible = visible;
        changed
    }
}

impl Crabdash {
    pub(crate) fn attach_dashboard_visibility(
        &mut self,
        window: &mut Window,
        minimised: bool,
        cx: &mut Context<Self>,
    ) {
        self.dashboard_visibility.visible = !minimised;
        self.dashboard_visibility.activation =
            Some(cx.observe_window_activation(window, |this, window, cx| {
                this.observe_dashboard_visibility(window, cx);
            }));
        if let Some(handle) = window.window_handle().downcast::<Self>() {
            self.start_system_resource_loop(handle, cx);
        }
    }

    pub(crate) fn observe_dashboard_visibility(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.dashboard_visibility.observe(
            super::platform::is_visible(window),
            window.is_window_active(),
        ) {
            self.dashboard_visibility_changed(cx);
        }
    }

    fn set_dashboard_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.dashboard_visibility.visible != visible {
            self.dashboard_visibility.visible = visible;
            self.dashboard_visibility_changed(cx);
        }
    }

    fn dashboard_visibility_changed(&mut self, cx: &mut Context<Self>) {
        // Pausing invalidates pending samples and counter baselines. Showing
        // starts a fresh baseline immediately, with a gap in existing history.
        self.prepare_system_resources(cx);
        if self.dashboard_visibility.is_visible() {
            self.prepare_visible_domains(cx);
            cx.notify();
        } else {
            self.polling.observe(
                &self.machine_store.machines[self.selected_machine],
                Domains::default(),
            );
        }
    }
}

pub(super) fn set_visible(window: &Window, cx: &mut App, visible: bool) {
    // Caption/action handlers can run while the root entity is already borrowed.
    // Defer its update until that handler returns.
    window.defer(cx, move |window, cx| {
        if let Some(Some(view)) = window.root::<Crabdash>() {
            view.update(cx, |this, cx| this.set_dashboard_visible(visible, cx));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::test;

    #[test]
    fn focus_loss_does_not_background_a_visible_dashboard() {
        let mut state = State::default();
        assert!(!state.observe(None, false));
        assert!(state.is_visible());
        state.visible = false;
        assert!(!state.observe(None, false));
        assert!(!state.is_visible());
        assert!(state.observe(None, true));
        assert!(state.is_visible());
    }

    #[test]
    fn native_visibility_overrides_focus_and_tracks_external_restore() {
        let mut state = State::default();
        assert!(state.observe(Some(false), true));
        assert!(!state.is_visible());
        assert!(!state.observe(Some(false), false));
        assert!(state.observe(Some(true), false));
        assert!(state.is_visible());
    }
}
