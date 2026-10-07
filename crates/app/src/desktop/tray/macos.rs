//! macOS uses the Dock lifecycle; no status tray backend is installed.
use super::TrayCommand;
use gpui::{App, Window};
use smol::channel::Receiver;
pub(super) const SUPPORTED: bool = false;
pub(crate) fn start(_: &mut App) -> Option<Receiver<TrayCommand>> {
    None
}
pub(crate) fn should_close(_: &mut Window, _: &mut App) -> bool {
    true
}
