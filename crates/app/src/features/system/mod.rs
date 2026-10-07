//! Live resource usage belongs to the selected machine, independently of tables.
mod chart;
mod clock;
mod controller;
mod history;
#[cfg(test)]
use history::HISTORY_LIMIT;
use history::{HistoryPoint, ScalarPoint, append};
mod processes;
mod status;
mod view;
pub(crate) use view::render;

use super::refresh::Requests;
use machines::resources::{ResourceMonitor, ResourceSample, ResourceUsage};
use machines::{machine::Machine, remote_connection::RemoteConnection};
use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
    time::{Duration, Instant},
};
use uuid::Uuid;

fn sum_rates(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let values: Vec<_> = values.collect();
    if values.is_empty() {
        return None;
    }
    values
        .into_iter()
        .try_fold(0.0, |sum, value| Some(sum + value?))
}
#[derive(Default)]
pub(crate) struct MachineState {
    target: Option<Target>,
    monitor: ResourceMonitor,
    boot_id: Option<String>,
    pub usage: Option<Rc<ResourceUsage>>,
    pub history: VecDeque<HistoryPoint>,
    pub gpu_history: HashMap<String, VecDeque<ScalarPoint>>,
    pub error: Option<String>,
    pub updated: Option<Instant>,
    pub loading: bool,
    last_requested: Option<Instant>,
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
    fn sampling_due(&self, now: Instant, interval: Duration) -> bool {
        !self.loading
            && self
                .last_requested
                .is_none_or(|requested| now.saturating_duration_since(requested) >= interval)
    }

    fn record_gap(&mut self, captured_at: Instant) {
        self.monitor.reset();
        append(&mut self.history, HistoryPoint::gap(captured_at));
        for history in self.gpu_history.values_mut() {
            append(
                history,
                ScalarPoint {
                    captured_at,
                    value: None,
                },
            );
        }
    }

    fn record(&mut self, sample: ResourceSample) -> anyhow::Result<()> {
        let boot_id = sample.boot_id.clone();
        let captured_at = sample.captured_at;
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
        append(
            &mut self.history,
            HistoryPoint {
                captured_at,
                cpu: usage.cpu_percent,
                memory: Some(usage.memory.used_percent()),
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
            },
        );
        if let Some(values) = &usage.gpus {
            self.gpu_history
                .retain(|id, _| values.iter().any(|value| &value.id == id));
            for value in values {
                append(
                    self.gpu_history.entry(value.id.clone()).or_default(),
                    ScalarPoint {
                        captured_at,
                        value: value.busy_percent,
                    },
                );
            }
        } else {
            for history in self.gpu_history.values_mut() {
                append(
                    history,
                    ScalarPoint {
                        captured_at,
                        value: None,
                    },
                );
            }
        }
        self.usage = Some(Rc::new(usage));
        self.error = None;
        self.updated = Some(Instant::now());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use machines::resources::{CpuSample, MemorySample};

    #[test]
    fn sample_schedule_respects_changed_intervals_pending_requests_and_failures() {
        let now = Instant::now();
        let mut state = MachineState::default();
        assert!(state.sampling_due(now, Duration::from_secs(2)));
        state.last_requested = Some(now);
        // A failed request has no updated sample, but should still be paced.
        state.error = Some("Disconnected".into());
        assert!(!state.sampling_due(now + Duration::from_secs(1), Duration::from_secs(2)));
        assert!(state.sampling_due(now + Duration::from_secs(2), Duration::from_secs(2)));
        assert!(!state.sampling_due(now + Duration::from_secs(2), Duration::from_secs(60)));
        state.loading = true;
        assert!(!state.sampling_due(now + Duration::from_secs(120), Duration::from_secs(2)));
    }

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
        assert_eq!(state.history[0].memory, Some(75.0));
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
        assert_eq!(state.gpu_history["gpu0"].len(), HISTORY_LIMIT);
        assert_eq!(
            state.gpu_history["gpu0"]
                .back()
                .and_then(|point| point.value),
            None
        );
        let mut removed = sample("boot", 102.0);
        removed.gpus = Some(vec![]);
        state.record(removed)?;
        assert!(state.gpu_history.is_empty());
        state.record(sample("newboot", 0.0))?;
        assert_eq!(state.history.len(), 1);
        Ok(())
    }

    #[test]
    fn failed_samples_preserve_values_but_break_every_history() -> anyhow::Result<()> {
        let mut state = MachineState::default();
        let start = Instant::now();
        let mut first = sample("boot", 0.0);
        first.captured_at = start;
        state.record(first)?;
        state.gpu_history.insert(
            "gpu".into(),
            VecDeque::from([ScalarPoint {
                captured_at: start,
                value: Some(25.0),
            }]),
        );
        let retained = state
            .usage
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Missing usage"))?;
        state.record_gap(start + Duration::from_secs(3));
        assert!(
            state
                .usage
                .as_ref()
                .is_some_and(|usage| Rc::ptr_eq(usage, &retained))
        );
        let gap = &state.history[1];
        assert_eq!(gap.captured_at, start + Duration::from_secs(3));
        assert!(gap.cpu.is_none() && gap.memory.is_none());
        assert!(gap.network_rx.is_none() && gap.network_tx.is_none());
        assert!(gap.disk_read.is_none() && gap.disk_write.is_none());
        assert_eq!(state.gpu_history["gpu"][1].value, None);
        assert!(state.usage.is_some());
        let mut next = sample("boot", 9.0);
        next.captured_at = start + Duration::from_secs(9);
        state.record(next)?;
        assert_eq!(state.history[2].captured_at, start + Duration::from_secs(9));
        assert_eq!(state.history[2].memory, Some(75.0));
        Ok(())
    }

    #[test]
    fn pausing_rejects_pending_samples_and_marks_a_history_gap() -> anyhow::Result<()> {
        let mut state = State::default();
        let uuid = Uuid::new_v4();
        let now = Instant::now();
        let ticket = state.requests.restart(uuid);
        let mut machine = MachineState::default();
        let mut first = sample("boot", 0.0);
        first.captured_at = now;
        machine.record(first)?;
        machine.loading = true;
        machine.last_requested = Some(now);
        state.machines.insert(uuid, machine);
        state.pause(uuid, now + Duration::from_secs(2));
        assert!(!state.requests.complete(&uuid, ticket));
        let machine = &state.machines[&uuid];
        assert!(machine.sampling_due(now + Duration::from_secs(2), Duration::from_secs(60)));
        assert_eq!(machine.history.back().and_then(|point| point.memory), None);
        assert!(machine.usage.is_some());
        Ok(())
    }

    #[test]
    fn interrupted_results_are_rejected_and_all_counter_rates_rebaseline() -> anyhow::Result<()> {
        use machines::resources::{
            CpuCoreCounter, CpuCounter, DiskCounter, NetworkCounter, ProcessCpu, ProcessSample,
            ProcessesSample,
        };
        let start = clock::Reading::now();
        let counters = |seconds, ticks| {
            let mut sample = sample("boot", seconds as f64);
            sample.captured_at = start.instant + Duration::from_secs(seconds);
            let counter = CpuCounter {
                total: ticks,
                idle: ticks / 4,
            };
            sample.cpu = CpuSample::Counters {
                aggregate: counter,
                cores: vec![CpuCoreCounter {
                    name: "0".into(),
                    counter,
                }],
            };
            sample.network = Some(vec![NetworkCounter {
                id: "eth0".into(),
                received_bytes: ticks,
                sent_bytes: ticks * 2,
            }]);
            sample.disks = Some(vec![DiskCounter {
                id: "disk".into(),
                read_bytes: ticks,
                written_bytes: ticks * 3,
            }]);
            sample.processes = Some(ProcessesSample {
                total_cpu: None,
                total_count: 1,
                truncated: false,
                entries: vec![ProcessSample {
                    pid: 42,
                    start_id: "first".into(),
                    name: "worker".into(),
                    user: None,
                    memory_bytes: None,
                    cpu: ProcessCpu::TimedCounter {
                        ticks,
                        ticks_per_second: 100,
                    },
                }],
            });
            sample
        };
        let mut state = State {
            clock: clock::SamplingClock::new(start),
            ..State::default()
        };
        let uuid = Uuid::new_v4();
        let mut machine = MachineState::default();
        machine.record(counters(0, 100))?;
        machine.record(counters(2, 300))?;
        assert_eq!(
            machine.history.back().and_then(|point| point.cpu),
            Some(75.0)
        );
        assert_eq!(
            machine.history.back().and_then(|point| point.network_rx),
            Some(100.0)
        );
        machine.loading = true;
        state.machines.insert(uuid, machine);
        let stale = state.requests.restart(uuid);
        // Both clocks count the interruption; a completion can arrive before a heartbeat.
        assert!(state.observe_clock(clock::Reading {
            instant: start.instant + Duration::from_secs(10),
            wall: start.wall + Duration::from_secs(10),
        }));
        let fresh = state.requests.begin(uuid);
        assert!(!state.requests.complete(&uuid, stale));
        assert!(fresh.is_some_and(|ticket| state.requests.complete(&uuid, ticket)));
        let machine = state
            .machines
            .get_mut(&uuid)
            .ok_or_else(|| anyhow::anyhow!("Missing fixture"))?;
        machine.record(counters(10, 10_000))?;
        let baseline = machine
            .usage
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing usage"))?;
        assert!(baseline.cpu_percent.is_none());
        assert!(
            baseline
                .network
                .as_ref()
                .is_some_and(|rows| rows[0].received_bytes_per_second.is_none())
        );
        assert!(
            baseline
                .disks
                .as_ref()
                .is_some_and(|rows| rows[0].written_bytes_per_second.is_none())
        );
        assert!(
            baseline
                .processes
                .as_ref()
                .is_some_and(|rows| rows[0].cpu_percent.is_none())
        );
        machine.record(counters(12, 10_200))?;
        let restored = machine
            .usage
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing usage"))?;
        assert_eq!(restored.cpu_percent, Some(75.0));
        assert_eq!(
            restored
                .network
                .as_ref()
                .and_then(|rows| rows[0].received_bytes_per_second),
            Some(100.0)
        );
        assert_eq!(
            restored
                .disks
                .as_ref()
                .and_then(|rows| rows[0].written_bytes_per_second),
            Some(300.0)
        );
        assert_eq!(
            restored
                .processes
                .as_ref()
                .and_then(|rows| rows[0].cpu_percent),
            Some(100.0)
        );
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
    clock: clock::SamplingClock,
    pub machines: HashMap<Uuid, MachineState>,
    visible_machine: Option<Uuid>,
    status: status::Tracker,
    pub processes: Option<processes::State>,
}
impl State {
    pub(crate) fn new(cx: &mut gpui::Context<crate::app::Crabdash>) -> Self {
        Self {
            processes: Some(processes::State::new(cx)),
            ..Self::default()
        }
    }
    fn observe_clock(&mut self, now: clock::Reading) -> bool {
        if !self.clock.observe(now) {
            return false;
        }
        let uuids: Vec<_> = self.machines.keys().copied().collect();
        for uuid in uuids {
            self.pause(uuid, now.instant);
        }
        true
    }

    fn pause(&mut self, uuid: Uuid, captured_at: Instant) {
        // Reject pending capture-start samples before adding the later gap.
        self.requests.forget(&uuid);
        if let Some(state) = self.machines.get_mut(&uuid) {
            state.loading = false;
            state.last_requested = None;
            if state.usage.is_some() {
                state.record_gap(captured_at);
            } else {
                state.monitor.reset();
            }
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
