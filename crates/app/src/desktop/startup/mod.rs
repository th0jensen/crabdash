//! Login startup controls the local desktop session only.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod runtime;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(any(target_os = "windows", test))]
mod windows_registration;
use anyhow::Result;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;
pub(crate) const SUPPORTED: bool = platform::SUPPORTED;
pub(crate) use runtime::{
    Runtime, begin_toggle, complete_toggle, initialize, refresh, set_start_minimised,
};
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(crate) struct LoginStartup {
    pub enabled: bool,
    pub start_minimised: bool,
    pub error: Option<String>,
}

impl LoginStartup {
    pub fn load() -> Self {
        let (start_minimised, preference_error) = match start_minimised() {
            Ok(value) => (value, None),
            Err(error) => (false, Some(error.to_string())),
        };
        match platform::startup_status() {
            Ok((enabled, warning)) => Self {
                enabled,
                start_minimised,
                error: preference_error.or(warning),
            },
            Err(error) => Self {
                enabled: false,
                start_minimised,
                error: Some(error.to_string()),
            },
        }
    }
}

/// Used by desktop launches and the systemd login service.
pub fn start_minimised() -> Result<bool> {
    Ok(crate::features::preferences::Preferences::load()?.start_minimised)
}

pub(crate) fn set_login_startup(enabled: bool) -> Result<()> {
    platform::set_login_startup(enabled)
}
