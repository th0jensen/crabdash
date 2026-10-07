use machines::store::MachineStore;
use uuid::Uuid;

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
    use super::reconcile;
    use machines::{machine::Machine, remote_connection::RemoteConnection, store::MachineStore};

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
}
