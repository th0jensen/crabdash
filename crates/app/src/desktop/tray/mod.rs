#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg_attr(
    target_os = "macos",
    expect(
        dead_code,
        reason = "Only the Linux status tray emits commands; macOS uses the Dock"
    )
)]
#[derive(Clone)]
pub enum TrayCommand {
    Show(Option<String>),
    Quit,
}

pub(crate) const SUPPORTED: bool = platform::SUPPORTED;
pub(crate) use platform::{should_close, start};
