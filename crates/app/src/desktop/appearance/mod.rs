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
pub(crate) use platform::{frame, frame_inset, root_background, titlebar_background};
#[cfg(target_os = "windows")]
use windows as platform;
