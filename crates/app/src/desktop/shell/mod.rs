//! The desktop owns navigation chrome; GPUI owns dashboard content.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::State;
#[cfg(target_os = "macos")]
pub(crate) use macos::commands_blocked as macos_popup_blocked;

pub(crate) fn is_native(app: &crate::Crabdash) -> bool {
    #[cfg(target_os = "macos")]
    {
        app.native_shell.is_some()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        false
    }
}

/// Width occupied by native navigation inside the shared GPUI viewport.
/// The terminal and full-window actions do not use this dashboard-only inset.
pub(crate) fn native_sidebar_inset(app: &crate::Crabdash) -> gpui::Pixels {
    #[cfg(target_os = "macos")]
    {
        app.native_shell
            .as_ref()
            .map_or(gpui::px(0.0), State::sidebar_inset)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        gpui::px(0.0)
    }
}

/// Modals keep their original detail-area centering even though GPUI now also
/// renders the terminal underneath native navigation.
pub(crate) fn overlay(app: &crate::Crabdash, element: impl gpui::IntoElement) -> gpui::Div {
    use gpui::prelude::*;
    gpui::div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(native_sidebar_inset(app))
        .right_0()
        .child(element)
}
