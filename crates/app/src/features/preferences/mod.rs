//! Preference persistence, editing, and application to the running UI.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
mod controller;
mod editor;
mod settings;
pub(crate) use editor::{Editor, render};
pub use settings::*;
