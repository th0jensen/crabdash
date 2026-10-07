//! Windows application menus use the shared compact in-window surface.
pub(crate) use super::in_window::{popup, render, visible};
use crate::app::Crabdash;
use gpui::*;
pub(super) fn append_app_items(_: &mut Vec<MenuItem>) {}
pub(super) fn register_actions(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("f10", crate::ToggleAppMenu, None),
        KeyBinding::new("ctrl-shift-m", crate::ZoomWindow, None),
    ]);
}
pub(super) fn shortcut(_macos: &'static str, linux: &'static str) -> &'static str {
    linux
}
pub(super) fn intercept(cx: &mut Context<Crabdash>) -> Option<Subscription> {
    let listener = cx.listener(|this, event: &KeystrokeEvent, window, cx| {
        if this.focus_handle.contains_focused(window, cx) {
            super::in_window::handle_key(this, &event.keystroke, window, cx);
        }
    });
    Some(cx.intercept_keystrokes(listener))
}

pub(super) fn platform_key() -> &'static str {
    "Win"
}
