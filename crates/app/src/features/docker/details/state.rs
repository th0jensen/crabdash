use crate::features::{polling::Target, refresh::Requests};
use machines::machine::Machine;
use std::collections::HashMap;
use utils::container::{Container, details::ContainerDetails};
use uuid::Uuid;

pub(crate) type Key = (Uuid, String);

pub(crate) enum Entry {
    Loading,
    Ready(ContainerDetails),
    Error(String),
}

#[derive(Default)]
pub(crate) struct State {
    entries: HashMap<Key, Entry>,
    requests: Requests<Key>,
    targets: HashMap<Key, Target>,
}

impl State {
    pub(crate) fn is_open(&self, key: &Key) -> bool {
        self.entries.contains_key(key)
    }
    pub(crate) fn get(&self, key: &Key) -> Option<&Entry> {
        self.entries.get(key)
    }
    pub(crate) fn begin(&mut self, key: Key) -> u64 {
        self.entries.insert(key.clone(), Entry::Loading);
        self.requests.restart(key)
    }
    pub(crate) fn record_target(&mut self, key: Key, target: Target) {
        self.targets.insert(key, target);
    }
    pub(crate) fn matches_target(&self, key: &Key, machine: &Machine) -> bool {
        self.targets
            .get(key)
            .is_some_and(|target| target.matches(machine))
    }
    pub(crate) fn complete(
        &mut self,
        key: &Key,
        ticket: u64,
        result: Result<ContainerDetails, String>,
    ) -> bool {
        if !self.is_open(key) || !self.requests.complete(key, ticket) {
            return false;
        }
        self.entries.insert(
            key.clone(),
            match result {
                Ok(details) => Entry::Ready(details),
                Err(message) => Entry::Error(message),
            },
        );
        true
    }
    /// Reject a stale target without disturbing a newer request for its key.
    pub(crate) fn discard(&mut self, key: &Key, ticket: u64) -> bool {
        if !self.requests.complete(key, ticket) {
            return false;
        }
        self.close(key);
        true
    }
    pub(crate) fn close(&mut self, key: &Key) {
        self.entries.remove(key);
        self.requests.forget(key);
        self.targets.remove(key);
    }
    pub(crate) fn remove_machine(&mut self, uuid: Uuid) {
        self.entries.retain(|key, _| key.0 != uuid);
        self.targets.retain(|key, _| key.0 != uuid);
        self.requests.forget_where(|key| key.0 == uuid);
    }
    pub(crate) fn reconcile_target(&mut self, machine: &Machine) {
        let stale: Vec<_> = self
            .targets
            .iter()
            .filter(|(key, target)| key.0 == machine.uuid && !target.matches(machine))
            .map(|(key, _)| key.clone())
            .collect();
        for key in stale {
            self.close(&key);
        }
    }
    pub(crate) fn retain_containers(&mut self, uuid: Uuid, containers: &[Container]) {
        let removed: Vec<_> = self
            .entries
            .keys()
            .filter(|key| {
                key.0 == uuid && !containers.iter().any(|container| container.id == key.1)
            })
            .cloned()
            .collect();
        for key in removed {
            self.close(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(machine: u128, id: &str) -> Key {
        (Uuid::from_u128(machine), id.to_string())
    }

    #[test]
    fn closing_and_reopening_rejects_old_completion_and_keeps_current_error() {
        let mut state = State::default();
        let key = key(1, "web");
        let old = state.begin(key.clone());
        state.close(&key);
        let current = state.begin(key.clone());
        assert!(!state.complete(&key, old, Err("old error".into())));
        assert!(matches!(state.get(&key), Some(Entry::Loading)));
        assert!(state.complete(&key, current, Err("permission denied".into())));
        assert!(
            matches!(state.get(&key), Some(Entry::Error(message)) if message == "permission denied")
        );
        assert!(!state.complete(&key, old, Err("late error".into())));
    }

    #[test]
    fn discarded_target_does_not_close_a_newer_request() {
        let mut state = State::default();
        let key = key(1, "web");
        let old = state.begin(key.clone());
        let current = state.begin(key.clone());
        assert!(!state.discard(&key, old));
        assert!(state.is_open(&key));
        assert!(state.discard(&key, current));
        assert!(!state.is_open(&key));
    }

    #[test]
    fn replaced_endpoint_or_session_closes_old_snapshot_and_invalidates_request() {
        use machines::remote_connection::RemoteConnection;
        let mut machine = Machine::default();
        machine.remote = Some(RemoteConnection::default());
        let key = (machine.uuid, "web".into());
        let mut state = State::default();
        let old = state.begin(key.clone());
        state.record_target(key.clone(), Target::from_machine(&machine));
        state.reconcile_target(&machine.clone());
        assert!(state.is_open(&key));
        if let Some(remote) = machine.remote.as_mut() {
            remote.host = "replacement".into();
        }
        state.reconcile_target(&machine);
        assert!(!state.is_open(&key));
        assert!(!state.complete(&key, old, Err("old endpoint".into())));
        let replaced = state.begin(key.clone());
        state.record_target(key.clone(), Target::from_machine(&machine));
        let mut remote = RemoteConnection::default();
        remote.host = "replacement".into();
        machine.remote = Some(remote);
        state.reconcile_target(&machine);
        assert!(!state.complete(&key, replaced, Err("old session".into())));
    }

    #[test]
    fn machine_removal_invalidates_only_its_requests() {
        let mut state = State::default();
        let removed = key(1, "web");
        let other = key(2, "web");
        let old = state.begin(removed.clone());
        let current = state.begin(other.clone());
        state.remove_machine(removed.0);
        assert!(!state.is_open(&removed));
        assert!(!state.complete(&removed, old, Err("removed".into())));
        assert!(state.complete(&other, current, Err("other".into())));
    }

    #[test]
    fn successful_list_prunes_disappeared_container_without_closing_survivors() {
        let mut state = State::default();
        let gone = key(1, "gone");
        let kept = key(1, "kept");
        let old = state.begin(gone.clone());
        let current = state.begin(kept.clone());
        state.retain_containers(
            gone.0,
            &[Container {
                id: "kept".into(),
                ..Default::default()
            }],
        );
        assert!(!state.complete(&gone, old, Err("gone".into())));
        assert!(state.complete(&kept, current, Err("kept".into())));
    }
}
