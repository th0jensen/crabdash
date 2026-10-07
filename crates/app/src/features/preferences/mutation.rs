//! Serialize settings writes across dashboards, including immediate login changes.
use gpui::{App, Global};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    LoginStartup,
    Preferences,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(crate) struct State {
    operation: Option<Operation>,
}
impl Global for State {}

impl State {
    fn begin(&mut self, operation: Operation) -> Result<(), &'static str> {
        match self.operation {
            Some(Operation::LoginStartup) => {
                Err("Login startup is being updated. Please try again shortly.")
            }
            Some(Operation::Preferences) => {
                Err("Another window is saving preferences. Please try again shortly.")
            }
            None => {
                self.operation = Some(operation);
                Ok(())
            }
        }
    }

    fn complete(&mut self, operation: Operation) {
        if self.operation == Some(operation) {
            self.operation = None;
        }
    }
}

fn publish(cx: &mut App, next: State) {
    if cx.try_global::<State>() != Some(&next) {
        cx.set_global(next);
    }
}

pub(crate) fn initialize(cx: &mut App) {
    if cx.try_global::<State>().is_none() {
        publish(cx, State::default());
    }
}

pub(crate) fn is_busy(cx: &App) -> bool {
    cx.try_global::<State>()
        .is_some_and(|state| state.operation.is_some())
}

pub(crate) fn begin(cx: &mut App, operation: Operation) -> Result<(), &'static str> {
    initialize(cx);
    let mut next = cx.global::<State>().clone();
    next.begin(operation)?;
    publish(cx, next);
    Ok(())
}

pub(crate) fn complete(cx: &mut App, operation: Operation) {
    let mut next = cx.global::<State>().clone();
    next.complete(operation);
    publish(cx, next);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_and_preference_writes_are_serialized_in_both_directions() -> anyhow::Result<()> {
        let mut state = State::default();
        state
            .begin(Operation::Preferences)
            .map_err(anyhow::Error::msg)?;
        assert!(state.begin(Operation::Preferences).is_err());
        assert!(state.begin(Operation::LoginStartup).is_err());
        state.complete(Operation::Preferences);
        state
            .begin(Operation::LoginStartup)
            .map_err(anyhow::Error::msg)?;
        assert!(state.begin(Operation::Preferences).is_err());
        assert!(state.begin(Operation::LoginStartup).is_err());
        // App-owned completion does not depend on the initiating owner surviving.
        state.complete(Operation::LoginStartup);
        state
            .begin(Operation::Preferences)
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }

    #[test]
    fn wrong_completion_cannot_release_another_operation() -> anyhow::Result<()> {
        let mut state = State::default();
        state
            .begin(Operation::LoginStartup)
            .map_err(anyhow::Error::msg)?;
        state.complete(Operation::Preferences);
        assert!(state.begin(Operation::Preferences).is_err());
        state.complete(Operation::LoginStartup);
        assert!(state.begin(Operation::Preferences).is_ok());
        Ok(())
    }
}
