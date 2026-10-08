//! Desktop materials and outer chrome, independent of feature presentation.
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
#[cfg(target_os = "macos")]
pub(super) use macos::prepare;
#[cfg(target_os = "macos")]
pub(crate) use macos::{sidebar_separator, toolbar_separator};
pub(crate) use platform::{frame, frame_inset, root_background, titlebar_background};
#[cfg(target_os = "windows")]
use windows as platform;

mod accent;
#[cfg(target_os = "linux")]
mod linux_accent;
#[cfg(target_os = "windows")]
mod windows_accent;
pub(crate) use accent::{initialize, system_accent};
