//! App-wide native login registration status and owner-independent completion.
use super::LoginStartup;
use gpui::{App, Global};

/// Every dashboard observes the same registration and operation. The operation
/// belongs to the application, so closing its initiating window cannot cancel it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Runtime {
    status: LoginStartup,
    busy: bool,
    operation_error: Option<String>,
}
impl Global for Runtime {}

impl Runtime {
    fn new(status: LoginStartup) -> Self {
        Self {
            status,
            busy: false,
            operation_error: None,
        }
    }

    pub(crate) fn status(&self) -> LoginStartup {
        let mut status = self.status.clone();
        if let Some(error) = &self.operation_error {
            status.error = Some(match &status.error {
                Some(warning) if warning != error => format!("{error}\n{warning}"),
                _ => error.clone(),
            });
        }
        status
    }

    pub(crate) fn startup_busy(&self) -> bool {
        self.busy
    }

    fn begin_toggle(&mut self) -> Result<bool, &'static str> {
        if self.busy {
            return Err("Login startup is being updated. Please try again shortly.");
        }
        self.busy = true;
        // A window's last-rendered switch may be stale. Always derive the next
        // setting from the shared, actual registration status.
        Ok(!self.status.enabled)
    }

    fn complete_toggle(&mut self, status: LoginStartup, error: Option<String>) {
        if self.busy {
            self.status = status;
            self.operation_error = error;
            self.busy = false;
        }
    }

    fn set_start_minimised(&mut self, value: bool) {
        self.status.start_minimised = value;
    }

    fn refresh(&mut self, status: LoginStartup) {
        // A status reload must not replace an in-flight registration or lose a
        // prior operation error before a confirmed subsequent change succeeds.
        if !self.busy {
            self.status = status;
        }
    }
}

fn publish(cx: &mut App, next: Runtime) {
    if cx.try_global::<Runtime>() != Some(&next) {
        cx.set_global(next);
    }
}

pub(crate) fn initialize(cx: &mut App) {
    if cx.try_global::<Runtime>().is_none() {
        publish(cx, Runtime::new(LoginStartup::load()));
    }
}

pub(crate) fn is_busy(cx: &App) -> bool {
    cx.try_global::<Runtime>()
        .is_some_and(|runtime| runtime.busy)
}

pub(crate) fn refresh(cx: &mut App) {
    initialize(cx);
    if !is_busy(cx) {
        let mut next = cx.global::<Runtime>().clone();
        next.refresh(LoginStartup::load());
        publish(cx, next);
    }
}

pub(crate) fn begin_toggle(cx: &mut App) -> Result<bool, &'static str> {
    initialize(cx);
    let mut next = cx.global::<Runtime>().clone();
    let enabled = next.begin_toggle()?;
    publish(cx, next);
    Ok(enabled)
}

pub(crate) fn complete_toggle(cx: &mut App, status: LoginStartup, error: Option<String>) {
    let mut next = cx.global::<Runtime>().clone();
    next.complete_toggle(status, error);
    publish(cx, next);
}

pub(crate) fn set_start_minimised(cx: &mut App, value: bool) {
    let mut next = cx.global::<Runtime>().clone();
    next.set_start_minimised(value);
    publish(cx, next);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(enabled: bool) -> LoginStartup {
        LoginStartup {
            enabled,
            start_minimised: false,
            error: None,
        }
    }

    #[test]
    fn second_window_toggles_the_completed_shared_status() -> anyhow::Result<()> {
        let mut runtime = Runtime::new(status(false));
        let stale_second_window = runtime.status();
        assert!(runtime.begin_toggle().map_err(anyhow::Error::msg)?);
        assert!(runtime.begin_toggle().is_err());
        runtime.complete_toggle(status(true), None);
        assert!(!stale_second_window.enabled);
        assert!(runtime.status().enabled);
        assert!(!runtime.begin_toggle().map_err(anyhow::Error::msg)?);
        Ok(())
    }

    #[test]
    fn reload_during_operation_keeps_new_windows_busy_and_retains_status() -> anyhow::Result<()> {
        let mut runtime = Runtime::new(status(false));
        runtime.begin_toggle().map_err(anyhow::Error::msg)?;
        runtime.refresh(status(true));
        let new_window = runtime.clone();
        assert!(new_window.startup_busy());
        assert!(!new_window.status().enabled);
        // Completion is app-owned and needs no reference to either window.
        runtime.complete_toggle(status(true), None);
        assert!(!runtime.startup_busy());
        assert!(runtime.status().enabled);
        assert!(runtime.begin_toggle().is_ok());
        Ok(())
    }

    #[test]
    fn failed_operation_keeps_actual_status_and_warning_until_success() -> anyhow::Result<()> {
        let mut runtime = Runtime::new(status(false));
        runtime.begin_toggle().map_err(anyhow::Error::msg)?;
        let mut actual = status(false);
        actual.error = Some("OS approval is required".into());
        runtime.complete_toggle(actual.clone(), Some("Registration failed".into()));
        assert!(!runtime.status().enabled);
        assert_eq!(
            runtime.status().error.as_deref(),
            Some("Registration failed\nOS approval is required")
        );
        runtime.refresh(actual);
        assert_eq!(
            runtime.status().error.as_deref(),
            Some("Registration failed\nOS approval is required")
        );
        runtime.begin_toggle().map_err(anyhow::Error::msg)?;
        runtime.complete_toggle(status(true), None);
        assert!(runtime.status().enabled);
        assert!(runtime.status().error.is_none());
        Ok(())
    }

    #[test]
    fn saved_start_minimised_preserves_registration_and_operation_error() -> anyhow::Result<()> {
        let mut runtime = Runtime::new(status(true));
        runtime.begin_toggle().map_err(anyhow::Error::msg)?;
        runtime.complete_toggle(status(true), Some("Could not unregister".into()));
        runtime.set_start_minimised(true);
        assert!(runtime.status().enabled);
        assert!(runtime.status().start_minimised);
        assert_eq!(
            runtime.status().error.as_deref(),
            Some("Could not unregister")
        );
        Ok(())
    }
}
