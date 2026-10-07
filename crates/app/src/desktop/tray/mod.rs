#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;
#[derive(Clone)]
pub enum TrayCommand {
    Show(Option<String>),
    Preferences(Option<String>),
    Quit,
}

pub(crate) const SUPPORTED: bool = platform::SUPPORTED;
pub(crate) use platform::{should_close, start};
