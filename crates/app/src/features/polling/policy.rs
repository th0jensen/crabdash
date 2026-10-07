use crate::features::workspaces::model::{Node, Tab};
use machines::{
    machine::{Machine, MachineKind},
    remote_connection::RemoteConnection,
};
use std::{
    collections::HashMap,
    mem::Discriminant,
    time::{Duration, Instant},
};
use uuid::Uuid;

const METADATA_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub(crate) struct Target {
    uuid: Uuid,
    kind: Discriminant<MachineKind>,
    remote: Option<RemoteConnection>,
}
impl Target {
    pub(crate) fn from_machine(machine: &Machine) -> Self {
        let mut remote = machine.remote.clone();
        if let Some(remote) = remote.as_mut() {
            remote.auth = None;
        }
        Self {
            uuid: machine.uuid,
            kind: std::mem::discriminant(&machine.kind),
            remote,
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.uuid == other.uuid
            && self.kind == other.kind
            && match (&self.remote, &other.remote) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.host == b.host && a.user == b.user && a.shares_session_with(b)
                }
                _ => false,
            }
    }
    pub(crate) fn matches(&self, machine: &Machine) -> bool {
        self.uuid == machine.uuid
            && self.kind == std::mem::discriminant(&machine.kind)
            && match (&self.remote, &machine.remote) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.host == b.host && a.user == b.user && a.shares_session_with(b)
                }
                _ => false,
            }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Domains(u8);
impl Domains {
    pub(crate) fn visible(node: &Node) -> Self {
        match node {
            Node::Pane { active, .. } => Self(Self::bit(*active)),
            Node::Split { first, second, .. } => {
                Self(Self::visible(first).0 | Self::visible(second).0)
            }
        }
    }
    fn bit(tab: Tab) -> u8 {
        match tab {
            Tab::Docker => 1,
            Tab::Disks => 2,
            Tab::Services => 4,
            Tab::System => 0,
        }
    }
    pub(crate) fn contains(self, tab: Tab) -> bool {
        self.0 & Self::bit(tab) != 0
    }
}

#[derive(Default)]
pub(crate) struct State {
    visible: Option<(Target, Domains)>,
    metadata: HashMap<Uuid, (Target, Instant)>,
    connections: HashMap<Uuid, (Target, bool)>,
}
impl State {
    pub(crate) fn replaced_target(&self, machine: &Machine) -> bool {
        self.visible
            .as_ref()
            .is_some_and(|(target, _)| target.uuid == machine.uuid && !target.matches(machine))
    }
    /// Commit the snapshot before callers launch reads; empty results and
    /// notifications must not retrigger an unchanged visible domain.
    pub(crate) fn observe(&mut self, machine: &Machine, domains: Domains) -> Domains {
        let target = Target::from_machine(machine);
        let newly_visible = match &self.visible {
            Some((old, previous)) if old.same(&target) => Domains(domains.0 & !previous.0),
            _ => domains,
        };
        self.visible = Some((target, domains));
        newly_visible
    }
    pub(crate) fn metadata_due(&self, machine: &Machine, now: Instant) -> bool {
        self.metadata
            .get(&machine.uuid)
            .is_none_or(|(target, requested)| {
                !target.matches(machine)
                    || now.saturating_duration_since(*requested) >= METADATA_INTERVAL
            })
    }
    pub(crate) fn metadata_replaced(&self, machine: &Machine) -> bool {
        self.metadata
            .get(&machine.uuid)
            .is_some_and(|(target, _)| !target.matches(machine))
    }
    pub(crate) fn record_metadata_request(&mut self, machine: &Machine, now: Instant) {
        self.metadata
            .insert(machine.uuid, (Target::from_machine(machine), now));
    }
    /// Remember exactly the connection state most recently published by the UI
    /// or heartbeat, including changes made through a shared SSH connection.
    pub(crate) fn publish_connection(&mut self, machine: &Machine) -> bool {
        let target = Target::from_machine(machine);
        let connected = machine.connected();
        let changed = self
            .connections
            .get(&machine.uuid)
            .is_none_or(|(old, previous)| !old.same(&target) || *previous != connected);
        self.connections.insert(machine.uuid, (target, connected));
        changed
    }
    pub(crate) fn forget(&mut self, uuid: Uuid) {
        self.metadata.remove(&uuid);
        self.connections.remove(&uuid);
        if self
            .visible
            .as_ref()
            .is_some_and(|(target, _)| target.uuid == uuid)
        {
            self.visible = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::workspaces::model::{Axis, Drop, Layout};

    #[test]
    fn nested_unfocused_panes_count_only_their_active_domains() {
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        assert!(layout.drop_tab(Tab::Disks, 1, Drop::Bottom));
        let domains = Domains::visible(&layout.root);
        assert!(domains.contains(Tab::Docker));
        assert!(domains.contains(Tab::Services));
        assert!(domains.contains(Tab::Disks));
        let machine = Machine::default();
        let mut state = State::default();
        assert_eq!(state.observe(&machine, domains), domains);
        assert!(layout.focus(1));
        assert_eq!(
            state.observe(&machine, Domains::visible(&layout.root)),
            Domains::default()
        );
    }

    #[test]
    fn hidden_domains_refresh_when_shown_again_even_with_empty_results() {
        let machine = Machine::default();
        let mut layout = Layout::default();
        let mut state = State::default();
        assert!(
            state
                .observe(&machine, Domains::visible(&layout.root))
                .contains(Tab::Docker)
        );
        // Re-rendering before or after completion does not launch a second read.
        assert_eq!(
            state.observe(&machine, Domains::visible(&layout.root)),
            Domains::default()
        );
        layout.select(Tab::Services);
        assert!(
            state
                .observe(&machine, Domains::visible(&layout.root))
                .contains(Tab::Services)
        );
        layout.select(Tab::Docker);
        assert!(machine.services.docker.is_empty());
        assert!(
            state
                .observe(&machine, Domains::visible(&layout.root))
                .contains(Tab::Docker)
        );
    }

    #[test]
    fn uuid_kind_endpoint_and_session_replacement_invalidate_visibility() {
        let domains = Domains(7);
        let mut machine = Machine::default();
        machine.remote = Some(RemoteConnection::default());
        let mut state = State::default();
        assert_eq!(state.observe(&machine, domains), domains);
        assert_eq!(state.observe(&machine.clone(), domains), Domains::default());
        assert!(!state.replaced_target(&machine.clone()));
        machine.uuid = Uuid::new_v4();
        assert!(!state.replaced_target(&machine));
        assert_eq!(state.observe(&machine, domains), domains);
        machine.kind = MachineKind::Linux;
        assert!(state.replaced_target(&machine));
        assert_eq!(state.observe(&machine, domains), domains);
        if let Some(remote) = &mut machine.remote {
            remote.host = "new-host".into();
        }
        assert_eq!(state.observe(&machine, domains), domains);
        if let Some(remote) = &mut machine.remote {
            remote.user = "new-user".into();
        }
        assert_eq!(state.observe(&machine, domains), domains);
        let mut replacement = RemoteConnection::default();
        replacement.host = "new-host".into();
        replacement.user = "new-user".into();
        machine.remote = Some(replacement);
        assert!(state.replaced_target(&machine));
        assert_eq!(state.observe(&machine, domains), domains);
        machine.remote = None;
        assert_eq!(state.observe(&machine, domains), domains);
    }

    #[test]
    fn system_only_has_no_periodic_table_reads() {
        let mut layout = Layout::default();
        assert!(layout.select(Tab::System));
        assert_eq!(Domains::visible(&layout.root), Domains::default());
        let root = Node::Split {
            id: 3,
            axis: Axis::Vertical,
            ratio: 0.5,
            first: Box::new(layout.root.clone()),
            second: Box::new(Node::Pane {
                id: 2,
                tabs: vec![Tab::Services],
                active: Tab::Services,
            }),
        };
        assert_eq!(Domains::visible(&root), Domains(4));
    }

    #[test]
    fn metadata_is_paced_from_requests_even_when_they_fail() {
        let now = Instant::now();
        let mut machine = Machine::default();
        let mut state = State::default();
        assert!(state.metadata_due(&machine, now));
        state.record_metadata_request(&machine, now);
        assert!(!state.metadata_due(&machine, now + Duration::from_secs(29)));
        assert!(state.metadata_due(&machine, now + Duration::from_secs(30)));
        machine.kind = MachineKind::Linux;
        assert!(state.metadata_due(&machine, now));
        state.record_metadata_request(&machine, now);
        state.forget(machine.uuid);
        assert!(state.metadata_due(&machine, now));
    }

    #[test]
    fn replacement_metadata_is_fresh_even_after_switching_targets() {
        let now = Instant::now();
        let mut machine = Machine::default();
        machine.remote = Some(RemoteConnection::default());
        let domains = Domains(1);
        let mut state = State::default();
        state.observe(&machine, domains);
        state.record_metadata_request(&machine, now);
        state.observe(&Machine::default(), domains);
        if let Some(remote) = &mut machine.remote {
            remote.host = "replacement".into();
        }
        // The current visible snapshot belongs to another UUID. Per-target
        // metadata still identifies the old in-flight owner when returning.
        assert!(!state.replaced_target(&machine));
        assert!(state.metadata_replaced(&machine));
        assert!(state.metadata_due(&machine, now + Duration::from_secs(1)));
        state.record_metadata_request(&machine, now + Duration::from_secs(1));
        assert!(!state.metadata_replaced(&machine));
        assert!(!state.metadata_due(&machine, now + Duration::from_secs(2)));
        assert!(state.metadata_due(&machine, now + Duration::from_secs(31)));
        let mut replacement = RemoteConnection::default();
        replacement.host = "replacement".into();
        machine.remote = Some(replacement);
        assert!(state.metadata_replaced(&machine));
        assert!(state.metadata_due(&machine, now + Duration::from_secs(2)));
    }

    #[test]
    fn recording_discovered_platform_paces_the_next_metadata_request() {
        let now = Instant::now();
        let mut machine = Machine::default();
        let mut state = State::default();
        state.record_metadata_request(&machine, now);
        machine.kind = MachineKind::Linux;
        assert!(state.metadata_due(&machine, now));
        state.record_metadata_request(&machine, now);
        assert!(!state.metadata_replaced(&machine));
        assert!(!state.metadata_due(&machine, now + Duration::from_secs(1)));
        assert!(state.metadata_due(&machine, now + Duration::from_secs(30)));
    }

    #[test]
    fn heartbeat_notifies_only_changed_published_connection_state() {
        let mut machine = Machine::default();
        machine.remote = Some(RemoteConnection::default());
        let mut state = State::default();
        assert!(state.publish_connection(&machine));
        assert!(!state.publish_connection(&machine));
        if let Some(remote) = &machine.remote {
            remote.set_connected(true);
        }
        assert!(state.publish_connection(&machine));
        assert!(!state.publish_connection(&machine));
    }
}
