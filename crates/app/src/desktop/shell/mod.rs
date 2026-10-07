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
