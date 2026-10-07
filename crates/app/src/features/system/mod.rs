//! Live resource usage belongs to the selected machine, independently of tables.
mod controller;
mod processes;
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
    pub network_rx: Option<f64>,
    pub network_tx: Option<f64>,
    pub disk_read: Option<f64>,
    pub disk_write: Option<f64>,
}
fn sum_rates(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let values: Vec<_> = values.collect();
    if values.is_empty() {
        return None;
    }
    values
        .into_iter()
        .try_fold(0.0, |sum, value| Some(sum + value?))
}
fn append<T>(history: &mut VecDeque<T>, value: T) {
    history.push_back(value);
    while history.len() > HISTORY_LIMIT {
        history.pop_front();
    }
}

#[derive(Default)]
pub(crate) struct MachineState {
    target: Option<Target>,
    monitor: ResourceMonitor,
    boot_id: Option<String>,
    pub usage: Option<ResourceUsage>,
    pub history: VecDeque<HistoryPoint>,
    pub gpu_history: HashMap<String, VecDeque<Option<f64>>>,
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
            self.gpu_history.clear();
        }
        self.boot_id = Some(boot_id);
        self.history.push_back(HistoryPoint {
            cpu: usage.cpu_percent,
            memory: usage.memory.used_percent(),
            network_rx: usage
                .network
                .as_ref()
                .and_then(|v| sum_rates(v.iter().map(|v| v.received_bytes_per_second))),
            network_tx: usage
                .network
                .as_ref()
                .and_then(|v| sum_rates(v.iter().map(|v| v.sent_bytes_per_second))),
            disk_read: usage
                .disks
                .as_ref()
                .and_then(|v| sum_rates(v.iter().map(|v| v.read_bytes_per_second))),
            disk_write: usage
                .disks
                .as_ref()
                .and_then(|v| sum_rates(v.iter().map(|v| v.written_bytes_per_second))),
        });
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        self.gpu_history.retain(|id, _| {
            usage
                .gpus
                .as_ref()
                .is_some_and(|values| values.iter().any(|value| &value.id == id))
        });
        if let Some(values) = &usage.gpus {
            for value in values {
                append(
                    self.gpu_history.entry(value.id.clone()).or_default(),
                    value.busy_percent,
                );
            }
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
    fn aggregate_io_and_gpu_histories_are_bounded_removed_and_reset_with_boot() -> anyhow::Result<()>
    {
        use machines::resources::{DiskCounter, GpuSample, NetworkCounter};
        let mut state = MachineState::default();
        let start = Instant::now();
        for index in 0..100 {
            let mut sample = sample("boot", index as f64);
            sample.captured_at = start + Duration::from_secs(index);
            sample.network = Some(vec![NetworkCounter {
                id: "eth0".into(),
                received_bytes: index * 100,
                sent_bytes: index * 200,
            }]);
            sample.disks = Some(vec![DiskCounter {
                id: "disk0".into(),
                read_bytes: index * 300,
                written_bytes: index * 400,
            }]);
            sample.gpus = Some(vec![GpuSample {
                id: "gpu0".into(),
                name: "GPU".into(),
                vendor: "AMD".into(),
                driver: None,
                busy_percent: Some(25.0),
                memory_used_bytes: None,
                memory_total_bytes: None,
                temperature_celsius: None,
            }]);
            state.record(sample)?;
        }
        assert_eq!(state.history.len(), HISTORY_LIMIT);
        assert_eq!(
            state.gpu_history.get("gpu0").map(VecDeque::len),
            Some(HISTORY_LIMIT)
        );
        assert_eq!(state.history.back().and_then(|v| v.network_rx), Some(100.0));
        assert_eq!(state.history.back().and_then(|v| v.disk_write), Some(400.0));
        state.record(sample("boot", 101.0))?;
        assert!(state.gpu_history.is_empty());
        state.record(sample("newboot", 0.0))?;
        assert_eq!(state.history.len(), 1);
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
    pub processes: Option<processes::State>,
}
impl State {
    pub(crate) fn new(cx: &mut gpui::Context<crate::app::Crabdash>) -> Self {
        Self {
            processes: Some(processes::State::new(cx)),
            ..Self::default()
        }
    }
    pub(crate) fn remove(&mut self, uuid: Uuid) {
        self.requests.forget(&uuid);
        self.machines.remove(&uuid);
        if self.visible_machine == Some(uuid) {
            self.visible_machine = None;
        }
    }
}
