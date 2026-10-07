//! Live resource usage belongs to the selected machine, independently of tables.
mod controller;
mod view;
pub(crate) use view::render;

use super::refresh::Requests;
use machines::resources::{ResourceMonitor, ResourceSample, ResourceUsage};
use machines::{machine::Machine, remote_connection::RemoteConnection};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub(crate) const INTERVAL: Duration = Duration::from_secs(2);
const HISTORY_LIMIT: usize = 60;

#[derive(Clone, Copy)]
pub(crate) struct HistoryPoint {
    pub cpu: Option<f64>,
    pub memory: f64,
}

#[derive(Default)]
pub(crate) struct MachineState {
    target: Option<Target>,
    monitor: ResourceMonitor,
    boot_id: Option<String>,
    pub usage: Option<ResourceUsage>,
    pub history: VecDeque<HistoryPoint>,
    pub error: Option<String>,
    pub updated: Option<Instant>,
    pub loading: bool,
}

#[derive(Clone)]
struct Target(Option<RemoteConnection>);
impl Target {
    fn from_machine(machine: &Machine) -> Self {
        let mut remote = machine.remote.clone();
        if let Some(remote) = remote.as_mut() {
            remote.auth = None;
        }
        Self(remote)
    }
    fn matches(&self, machine: &Machine) -> bool {
        match (&self.0, &machine.remote) {
            (None, None) => true,
            (Some(old), Some(current)) => {
                old.host == current.host
                    && old.user == current.user
                    && old.shares_session_with(current)
            }
            _ => false,
        }
    }
}

impl MachineState {
    fn record(&mut self, sample: ResourceSample) -> anyhow::Result<()> {
        let boot_id = sample.boot_id.clone();
        let usage = self.monitor.update(sample)?;
        if self
            .boot_id
            .as_ref()
            .is_some_and(|current| current != &boot_id)
            || self
                .usage
                .as_ref()
                .is_some_and(|previous| usage.uptime_seconds < previous.uptime_seconds)
        {
            self.history.clear();
        }
        self.boot_id = Some(boot_id);
        self.history.push_back(HistoryPoint {
            cpu: usage.cpu_percent,
            memory: usage.memory.used_percent(),
        });
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        self.usage = Some(usage);
        self.error = None;
        self.updated = Some(Instant::now());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use machines::resources::{CpuSample, MemorySample};

    fn sample(boot_id: &str, uptime_seconds: f64) -> ResourceSample {
        ResourceSample {
            cpu: CpuSample::Sampled { percent: 25.0 },
            logical_cpus: 1,
            memory: MemorySample {
                total_bytes: 4096,
                available_bytes: 1024,
                estimated: false,
            },
            swap: None,
            load_average: None,
            uptime_seconds,
            boot_id: boot_id.into(),
            captured_at: Instant::now(),
            processes: None,
            network: None,
            disks: None,
            gpus: None,
        }
    }

    #[test]
    fn history_is_bounded_and_cleared_when_the_machine_restarts() -> anyhow::Result<()> {
        let mut state = MachineState::default();
        for index in 0..100 {
            state.record(sample("boot", index as f64))?;
        }
        assert_eq!(state.history.len(), HISTORY_LIMIT);
        state.record(sample("new-boot", 0.0))?;
        assert_eq!(state.history.len(), 1);
        state.record(sample("new-boot", 10.0))?;
        state.record(sample("new-boot", 1.0))?;
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].cpu, Some(25.0));
        assert_eq!(state.history[0].memory, 75.0);
        Ok(())
    }

    #[test]
    fn removing_a_machine_rejects_its_pending_sample_and_removes_history() {
        let mut state = State::default();
        let uuid = Uuid::new_v4();
        let ticket = state.requests.restart(uuid);
        state.visible_machine = Some(uuid);
        state.machines.insert(uuid, MachineState::default());
        state.remove(uuid);
        assert!(!state.requests.complete(&uuid, ticket));
        assert!(!state.machines.contains_key(&uuid));
        assert_eq!(state.visible_machine, None);
    }

    #[test]
    fn resource_identity_follows_the_endpoint_and_shared_runtime_session() {
        let mut machine = Machine::default();
        assert!(Target::from_machine(&machine).matches(&machine.clone()));
        machine.remote = Some(RemoteConnection::default());
        let target = Target::from_machine(&machine);
        assert!(target.matches(&machine.clone()));
        let mut changed = machine.clone();
        changed.remote = Some(RemoteConnection::default());
        assert!(!target.matches(&changed));
        changed = machine.clone();
        if let Some(remote) = changed.remote.as_mut() {
            remote.host = "new-host".into();
        }
        assert!(!target.matches(&changed));
        changed = machine;
        changed.remote = None;
        assert!(!target.matches(&changed));
    }
}

#[derive(Default)]
pub(crate) struct State {
    requests: Requests,
    pub machines: HashMap<Uuid, MachineState>,
    visible_machine: Option<Uuid>,
}
impl State {
    pub(crate) fn remove(&mut self, uuid: Uuid) {
        self.requests.forget(&uuid);
        self.machines.remove(&uuid);
        if self.visible_machine == Some(uuid) {
            self.visible_machine = None;
        }
    }
}
