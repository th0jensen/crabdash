use machines::store::MachineStore;
use uuid::Uuid;

/// An asynchronous add may select its result only while the user's selection
/// still has the identity and generation present when they submitted it.
#[derive(Clone, Copy)]
pub(super) struct Selection {
    pub uuid: Uuid,
    pub generation: u64,
}

impl Selection {
    pub fn permits_added_selection(self, current: Self) -> bool {
        self.uuid == current.uuid && self.generation == current.generation
    }
}

/// Keep runtime sessions and cached data when a persisted list changes order.
pub(crate) fn reconcile(
    incoming: &mut MachineStore,
    current: &MachineStore,
    selected: Uuid,
) -> usize {
    for machine in &mut incoming.machines {
        if let Some(previous) = current.machines.iter().find(|m| m.uuid == machine.uuid) {
            let same_target = match (&machine.remote, &previous.remote) {
                (None, None) => true,
                (Some(new), Some(old)) => new.host == old.host && new.user == old.user,
                _ => false,
            };
            if same_target {
                machine.services = previous.services.clone();
                if machine.docker_path.is_none() {
                    machine.docker_path = previous.docker_path.clone();
                }
                if let (Some(new), Some(old)) = (&mut machine.remote, &previous.remote) {
                    new.restore_session_from(old);
                    new.auth = old.auth.clone();
                }
            }
        }
    }
    incoming
        .machines
        .iter()
        .position(|m| m.uuid == selected)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{Selection, reconcile};
    use machines::{machine::Machine, remote_connection::RemoteConnection, store::MachineStore};

    #[test]
    fn async_add_never_overrides_a_newer_selection_intent_or_identity() {
        let submitted = Selection {
            uuid: uuid::Uuid::new_v4(),
            generation: 10,
        };
        assert!(submitted.permits_added_selection(submitted));
        assert!(!submitted.permits_added_selection(Selection {
            generation: 11,
            ..submitted
        }));
        assert!(!submitted.permits_added_selection(Selection {
            uuid: uuid::Uuid::new_v4(),
            ..submitted
        }));
    }

    #[test]
    fn reordered_or_unselected_deletion_preserves_selection_but_selected_delete_transitions() {
        let first = Machine::default();
        let selected = Machine::default();
        let last = Machine::default();
        let current = MachineStore {
            machines: vec![first.clone(), selected.clone(), last.clone()],
        };
        let before = Selection {
            uuid: selected.uuid,
            generation: 20,
        };
        let mut incoming = MachineStore {
            machines: vec![last.clone(), selected.clone()],
        };
        let index = reconcile(&mut incoming, &current, before.uuid);
        assert_eq!(before.uuid, incoming.machines[index].uuid);
        let mut incoming = MachineStore {
            machines: vec![first.clone(), last],
        };
        let index = reconcile(&mut incoming, &current, before.uuid);
        assert_ne!(before.uuid, incoming.machines[index].uuid);
        assert_eq!(incoming.machines[index].uuid, first.uuid);
    }

    #[test]
    fn selection_and_runtime_follow_identity_after_removal() {
        let removed = Machine::default();
        let mut kept = Machine::default();
        kept.remote = Some(RemoteConnection::default());
        if let Some(remote) = &kept.remote {
            remote.set_connected(true);
        }
        kept.services.systemd_error = Some("cached error".into());
        let current = MachineStore {
            machines: vec![removed, kept.clone()],
        };
        let mut reloaded = kept.clone();
        reloaded.services = Default::default();
        reloaded.remote = Some(RemoteConnection::default());
        let mut incoming = MachineStore {
            machines: vec![reloaded],
        };
        assert_eq!(reconcile(&mut incoming, &current, kept.uuid), 0);
        assert!(incoming.machines[0].connected());
        assert_eq!(
            incoming.machines[0].services.systemd_error.as_deref(),
            Some("cached error")
        );
    }

    #[test]
    fn changed_endpoint_does_not_inherit_a_session() {
        let mut previous = Machine::default();
        previous.remote = Some(RemoteConnection::default());
        if let Some(remote) = &previous.remote {
            remote.set_connected(true);
        }
        let mut changed = previous.clone();
        let mut new_remote = RemoteConnection::default();
        new_remote.host = "new-host".into();
        changed.remote = Some(new_remote);
        let mut incoming = MachineStore {
            machines: vec![changed],
        };
        reconcile(
            &mut incoming,
            &MachineStore {
                machines: vec![previous.clone()],
            },
            previous.uuid,
        );
        assert!(!incoming.machines[0].connected());
    }

    #[test]
    fn renamed_saved_alias_preserves_runtime_session_credentials_and_cache() {
        let mut previous = Machine::default();
        let mut remote = RemoteConnection::default();
        remote.user = "operator".into();
        remote.host = "example.invalid".into();
        remote.auth = Some(machines::remote_connection::AuthMethod::Password(
            "session-secret".into(),
        ));
        remote.set_connected(true);
        previous.remote = Some(remote);
        previous.services.systemd_error = Some("existing cache".into());
        let mut renamed = previous.clone();
        renamed.alias = Some("My renamed server".into());
        renamed.remote = Some(RemoteConnection::default());
        if let Some(remote) = &mut renamed.remote {
            remote.user = "operator".into();
            remote.host = "example.invalid".into();
        }
        let mut incoming = MachineStore {
            machines: vec![renamed],
        };
        let current = MachineStore {
            machines: vec![previous.clone()],
        };
        assert_eq!(reconcile(&mut incoming, &current, previous.uuid), 0);
        let renamed = &incoming.machines[0];
        assert_eq!(renamed.display_name(), "My renamed server");
        assert_eq!(renamed.id, previous.id);
        assert!(renamed.connected());
        assert_eq!(
            renamed.services.systemd_error.as_deref(),
            Some("existing cache")
        );
        assert!(
            matches!(renamed.remote.as_ref().and_then(|remote| remote.auth.as_ref()), Some(machines::remote_connection::AuthMethod::Password(secret)) if secret == "session-secret")
        );
    }
}
