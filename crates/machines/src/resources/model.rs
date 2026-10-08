//! Platform-neutral samples and interval CPU calculations. Live data is never persisted.
use super::{
    DiskCounter, DiskUsage, GpuSample, NetworkCounter, NetworkUsage, ProcessUsage, ProcessesSample,
    processes,
};
use anyhow::{Result, ensure};
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuCounter {
    pub total: u64,
    pub idle: u64,
}

impl CpuCounter {
    fn is_monotonic_since(self, previous: Self) -> bool {
        self.total >= previous.total
            && self.idle >= previous.idle
            && self.total - self.idle >= previous.total - previous.idle
    }

    fn usage_since(self, previous: Self) -> Option<f64> {
        let total = self.total.checked_sub(previous.total)?;
        let idle = self.idle.checked_sub(previous.idle)?;
        if total == 0 || idle > total {
            return None;
        }
        Some((total - idle) as f64 / total as f64 * 100.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpuCoreCounter {
    pub name: String,
    pub counter: CpuCounter,
}

#[derive(Clone, Debug)]
pub enum CpuSample {
    Counters {
        aggregate: CpuCounter,
        cores: Vec<CpuCoreCounter>,
    },
    /// Built-in macOS top measures an interval on the target itself. It does
    /// not expose cumulative/per-core counters through its command-line output.
    Sampled { percent: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemorySample {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub estimated: bool,
}
impl MemorySample {
    pub fn used_bytes(self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }
    pub fn used_percent(self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.used_bytes() as f64 / self.total_bytes as f64 * 100.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapSample {
    pub total_bytes: u64,
    pub free_bytes: u64,
}
impl SwapSample {
    pub fn used_bytes(self) -> u64 {
        self.total_bytes.saturating_sub(self.free_bytes)
    }
}

#[derive(Clone, Debug)]
pub struct ResourceSample {
    pub cpu: CpuSample,
    pub logical_cpus: usize,
    pub memory: MemorySample,
    pub swap: Option<SwapSample>,
    pub load_average: Option<[f64; 3]>,
    pub uptime_seconds: f64,
    pub boot_id: String,
    /// Local monotonic capture time. Never persisted or derived from a poll preference.
    pub captured_at: Instant,
    pub processes: Option<ProcessesSample>,
    pub network: Option<Vec<NetworkCounter>>,
    pub disks: Option<Vec<DiskCounter>>,
    pub gpus: Option<Vec<GpuSample>>,
}
impl ResourceSample {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.logical_cpus > 0 && self.logical_cpus <= 65536,
            "Invalid logical CPU count"
        );
        ensure!(
            self.memory.total_bytes > 0 && self.memory.available_bytes <= self.memory.total_bytes,
            "Invalid resource memory values"
        );
        if let Some(swap) = self.swap {
            ensure!(
                swap.free_bytes <= swap.total_bytes,
                "Invalid resource swap values"
            );
        }
        ensure!(
            self.uptime_seconds.is_finite() && self.uptime_seconds >= 0.0,
            "Invalid resource uptime"
        );
        if let Some(load) = self.load_average {
            ensure!(
                load.iter().all(|value| value.is_finite() && *value >= 0.0),
                "Invalid load average"
            );
        }
        match &self.cpu {
            CpuSample::Sampled { percent } => ensure!(
                percent.is_finite() && (0.0..=100.0).contains(percent),
                "Invalid sampled CPU usage"
            ),
            CpuSample::Counters { aggregate, cores } => {
                ensure!(
                    aggregate.idle <= aggregate.total,
                    "Invalid aggregate CPU counter"
                );
                let mut names = HashSet::new();
                ensure!(
                    cores.len() == self.logical_cpus,
                    "Resource CPU topology is incomplete"
                );
                for core in cores {
                    ensure!(
                        core.counter.idle <= core.counter.total,
                        "Invalid per-core CPU counter"
                    );
                    ensure!(
                        !core.name.is_empty() && names.insert(&core.name),
                        "Duplicate resource CPU identifier"
                    );
                }
            }
        }
        ensure!(!self.boot_id.is_empty(), "Missing resource boot identity");
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CpuUsage {
    pub name: String,
    pub percent: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct ResourceUsage {
    pub cpu_percent: Option<f64>,
    pub cores: Vec<CpuUsage>,
    pub logical_cpus: usize,
    pub memory: MemorySample,
    pub swap: Option<SwapSample>,
    pub load_average: Option<[f64; 3]>,
    pub uptime_seconds: f64,
    pub processes: Option<Vec<ProcessUsage>>,
    pub process_count: Option<usize>,
    pub processes_truncated: bool,
    pub network: Option<Vec<NetworkUsage>>,
    pub disks: Option<Vec<DiskUsage>>,
    pub gpus: Option<Vec<GpuSample>>,
}

fn rate(new: u64, old: Option<u64>, seconds: Option<f64>) -> Option<f64> {
    let delta = new.checked_sub(old?)?;
    Some(delta as f64 / seconds?)
}

#[derive(Default)]
pub struct ResourceMonitor {
    previous: Option<ResourceSample>,
}
impl ResourceMonitor {
    pub fn reset(&mut self) {
        self.previous = None;
    }
    pub fn update(&mut self, sample: ResourceSample) -> Result<ResourceUsage> {
        sample.validate()?;
        let previous = self.previous.as_ref().filter(|previous| {
            previous.boot_id == sample.boot_id
                && previous.uptime_seconds <= sample.uptime_seconds
                && previous.logical_cpus == sample.logical_cpus
        });
        let (cpu_percent, cores) = match &sample.cpu {
            CpuSample::Sampled { percent } => (Some(*percent), Vec::new()),
            CpuSample::Counters { aggregate, cores } => {
                let previous_counters = previous.and_then(|previous| match &previous.cpu {
                    CpuSample::Counters {
                        aggregate: old,
                        cores: old_cores,
                    } if cores.iter().zip(old_cores).all(|(new, old)| {
                        new.name == old.name && new.counter.is_monotonic_since(old.counter)
                    }) =>
                    {
                        Some((*old, old_cores))
                    }
                    _ => None,
                });
                let cpu = previous_counters.and_then(|(old, _)| aggregate.usage_since(old));
                // A reset of any core rebaselines the entire CPU snapshot: its
                // regression can otherwise be hidden by other cores' increases
                // in the aggregate. Unchanged cores may still have no interval.
                let cores = cores
                    .iter()
                    .enumerate()
                    .map(|(index, core)| CpuUsage {
                        name: core.name.clone(),
                        percent: cpu.and_then(|_| {
                            previous_counters
                                .and_then(|(_, old)| old.get(index))
                                .and_then(|old| core.counter.usage_since(old.counter))
                        }),
                    })
                    .collect();
                (cpu, cores)
            }
        };
        let seconds = previous
            .and_then(|previous| {
                sample
                    .captured_at
                    .checked_duration_since(previous.captured_at)
            })
            .map(|elapsed| elapsed.as_secs_f64())
            .filter(|value| value.is_finite() && *value > 0.0);
        let old_network: HashMap<_, _> = previous
            .and_then(|p| p.network.as_ref())
            .into_iter()
            .flatten()
            .map(|counter| (counter.id.as_str(), counter))
            .collect();
        let network = sample.network.as_ref().map(|counters| {
            counters
                .iter()
                .map(|counter| {
                    let old = old_network.get(counter.id.as_str());
                    NetworkUsage {
                        id: counter.id.clone(),
                        received_bytes_per_second: rate(
                            counter.received_bytes,
                            old.map(|c| c.received_bytes),
                            seconds,
                        ),
                        sent_bytes_per_second: rate(
                            counter.sent_bytes,
                            old.map(|c| c.sent_bytes),
                            seconds,
                        ),
                    }
                })
                .collect()
        });
        let old_disks: HashMap<_, _> = previous
            .and_then(|p| p.disks.as_ref())
            .into_iter()
            .flatten()
            .map(|counter| (counter.id.as_str(), counter))
            .collect();
        let disks = sample.disks.as_ref().map(|counters| {
            counters
                .iter()
                .map(|counter| {
                    let old = old_disks.get(counter.id.as_str());
                    DiskUsage {
                        id: counter.id.clone(),
                        read_bytes_per_second: rate(
                            counter.read_bytes,
                            old.map(|c| c.read_bytes),
                            seconds,
                        ),
                        written_bytes_per_second: rate(
                            counter.written_bytes,
                            old.map(|c| c.written_bytes),
                            seconds,
                        ),
                    }
                })
                .collect()
        });
        let usage = ResourceUsage {
            cpu_percent,
            cores,
            logical_cpus: sample.logical_cpus,
            memory: sample.memory,
            swap: sample.swap,
            load_average: sample.load_average,
            uptime_seconds: sample.uptime_seconds,
            processes: sample.processes.as_ref().map(|p| {
                processes::usage(
                    p,
                    previous.and_then(|old| old.processes.as_ref()),
                    seconds,
                    sample.logical_cpus,
                )
            }),
            process_count: sample.processes.as_ref().map(|p| p.total_count),
            processes_truncated: sample.processes.as_ref().is_some_and(|p| p.truncated),
            network,
            disks,
            gpus: sample.gpus.clone(),
        };
        self.previous = Some(sample);
        Ok(usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(total: u64, idle: u64, uptime: f64) -> ResourceSample {
        let counter = CpuCounter { total, idle };
        ResourceSample {
            cpu: CpuSample::Counters {
                aggregate: counter,
                cores: vec![CpuCoreCounter {
                    name: "0".into(),
                    counter,
                }],
            },
            logical_cpus: 1,
            memory: MemorySample {
                total_bytes: 4096,
                available_bytes: 1024,
                estimated: false,
            },
            swap: None,
            load_average: Some([0.0; 3]),
            uptime_seconds: uptime,
            boot_id: "boot".into(),
            captured_at: Instant::now(),
            processes: None,
            network: None,
            disks: None,
            gpus: None,
        }
    }
    #[test]
    fn interval_cpu_requires_two_samples_and_resets_after_restart() -> Result<()> {
        let mut monitor = ResourceMonitor::default();
        assert_eq!(monitor.update(sample(100, 40, 10.0))?.cpu_percent, None);
        let second = monitor.update(sample(200, 60, 11.0))?;
        assert_eq!(second.cpu_percent, Some(80.0));
        assert_eq!(second.cores[0].percent, Some(80.0));
        assert_eq!(second.memory.used_bytes(), 3072);
        assert_eq!(second.memory.used_percent(), 75.0);
        assert_eq!(monitor.update(sample(200, 60, 12.0))?.cpu_percent, None);
        assert_eq!(monitor.update(sample(10, 5, 1.0))?.cpu_percent, None);
        assert_eq!(monitor.update(sample(20, 10, 2.0))?.cpu_percent, Some(50.0));
        let mut reboot = sample(40, 15, 3.0);
        reboot.boot_id = "different".into();
        assert_eq!(monitor.update(reboot)?.cpu_percent, None);
        monitor.reset();
        assert_eq!(monitor.update(sample(50, 20, 4.0))?.cpu_percent, None);
        Ok(())
    }
    #[test]
    fn counter_regression_and_cpu_reordering_rebaseline_instead_of_spiking() -> Result<()> {
        let mut monitor = ResourceMonitor::default();
        monitor.update(sample(100, 40, 10.0))?;
        assert_eq!(monitor.update(sample(200, 30, 11.0))?.cpu_percent, None);
        assert_eq!(
            monitor.update(sample(300, 50, 12.0))?.cpu_percent,
            Some(80.0)
        );
        let mut renamed = sample(400, 70, 13.0);
        if let CpuSample::Counters { cores, .. } = &mut renamed.cpu {
            cores[0].name = "1".into();
        }
        assert_eq!(monitor.update(renamed)?.cpu_percent, None);
        let mut invalid = sample(500, 100, f64::NAN);
        assert!(monitor.update(invalid.clone()).is_err());
        invalid.uptime_seconds = 15.0;
        invalid.memory.available_bytes = 5000;
        assert!(monitor.update(invalid).is_err());
        Ok(())
    }

    #[test]
    fn individual_core_regressions_rebaseline_even_when_aggregate_increases() -> Result<()> {
        let two_cores = |a: CpuCounter, b: CpuCounter, uptime| {
            let mut sample = sample(a.total + b.total, a.idle + b.idle, uptime);
            sample.logical_cpus = 2;
            sample.cpu = CpuSample::Counters {
                aggregate: CpuCounter {
                    total: a.total + b.total,
                    idle: a.idle + b.idle,
                },
                cores: vec![
                    CpuCoreCounter {
                        name: "0".into(),
                        counter: a,
                    },
                    CpuCoreCounter {
                        name: "1".into(),
                        counter: b,
                    },
                ],
            };
            sample
        };
        for (before, after) in [
            // A u32 Mach tick counter wraps, masked by another core increasing.
            (
                CpuCounter {
                    total: u32::MAX as u64,
                    idle: 10,
                },
                CpuCounter {
                    total: 20,
                    idle: 15,
                },
            ),
            // Idle regresses while total increases.
            (
                CpuCounter {
                    total: 100,
                    idle: 40,
                },
                CpuCounter {
                    total: 110,
                    idle: 30,
                },
            ),
            // Busy regresses while total and idle both increase.
            (
                CpuCounter {
                    total: 100,
                    idle: 40,
                },
                CpuCounter {
                    total: 110,
                    idle: 60,
                },
            ),
        ] {
            let mut monitor = ResourceMonitor::default();
            let other_before = CpuCounter {
                total: 100,
                idle: 20,
            };
            let other_after = CpuCounter {
                total: u32::MAX as u64 + 200,
                idle: 100,
            };
            monitor.update(two_cores(before, other_before, 10.0))?;
            let reset = monitor.update(two_cores(after, other_after, 11.0))?;
            assert_eq!(reset.cpu_percent, None);
            assert!(reset.cores.iter().all(|core| core.percent.is_none()));
            let recovered = monitor.update(two_cores(
                CpuCounter {
                    total: after.total + 100,
                    idle: after.idle + 20,
                },
                CpuCounter {
                    total: other_after.total + 100,
                    idle: other_after.idle + 20,
                },
                12.0,
            ))?;
            assert_eq!(recovered.cpu_percent, Some(80.0));
            assert!(
                recovered
                    .cores
                    .iter()
                    .all(|core| core.percent == Some(80.0))
            );
        }
        Ok(())
    }

    #[test]
    fn unchanged_core_does_not_erase_other_core_interval() -> Result<()> {
        let mut monitor = ResourceMonitor::default();
        let mut first = sample(200, 80, 10.0);
        first.logical_cpus = 2;
        if let CpuSample::Counters { cores, .. } = &mut first.cpu {
            cores[0].counter = CpuCounter {
                total: 100,
                idle: 40,
            };
            cores.push(CpuCoreCounter {
                name: "1".into(),
                counter: CpuCounter {
                    total: 100,
                    idle: 40,
                },
            });
        }
        let mut next = first.clone();
        next.uptime_seconds = 11.0;
        if let CpuSample::Counters { aggregate, cores } = &mut next.cpu {
            aggregate.total += 100;
            aggregate.idle += 20;
            cores[1].counter.total += 100;
            cores[1].counter.idle += 20;
        }
        monitor.update(first)?;
        let usage = monitor.update(next)?;
        assert_eq!(usage.cpu_percent, Some(80.0));
        assert_eq!(usage.cores[0].percent, None);
        assert_eq!(usage.cores[1].percent, Some(80.0));
        Ok(())
    }

    #[test]
    fn invalid_samples_preserve_baseline_and_topology_changes_reset_it() -> Result<()> {
        let mut monitor = ResourceMonitor::default();
        monitor.update(sample(100, 40, 10.0))?;
        let mut invalid = sample(200, 60, 11.0);
        invalid.load_average = Some([f64::INFINITY, 0.0, 0.0]);
        assert!(monitor.update(invalid).is_err());
        assert_eq!(
            monitor.update(sample(300, 80, 12.0))?.cpu_percent,
            Some(80.0)
        );
        let mut topology = sample(400, 100, 13.0);
        topology.logical_cpus = 2;
        if let CpuSample::Counters { cores, .. } = &mut topology.cpu {
            cores.push(CpuCoreCounter {
                name: "1".into(),
                counter: CpuCounter {
                    total: 100,
                    idle: 50,
                },
            });
        }
        let updated = monitor.update(topology)?;
        assert_eq!(updated.cpu_percent, None);
        assert!(updated.cores.iter().all(|core| core.percent.is_none()));
        Ok(())
    }
    #[test]
    fn io_uses_actual_elapsed_time_and_rebaselines_identity_reset_and_reboot() -> Result<()> {
        let mut monitor = ResourceMonitor::default();
        let start = Instant::now();
        let io = |bytes, elapsed, uptime| {
            let mut sample = sample(100 + bytes, 40, uptime);
            sample.captured_at = start + std::time::Duration::from_secs(elapsed);
            sample.network = Some(vec![NetworkCounter {
                id: "eth0".into(),
                received_bytes: bytes,
                sent_bytes: bytes * 2,
            }]);
            sample.disks = Some(vec![DiskCounter {
                id: "nvme0n1".into(),
                read_bytes: bytes,
                written_bytes: bytes * 3,
            }]);
            sample
        };
        let first = monitor.update(io(100, 0, 10.0))?;
        assert_eq!(
            first
                .network
                .as_ref()
                .and_then(|v| v.first())
                .and_then(|v| v.received_bytes_per_second),
            None
        );
        let second = monitor.update(io(400, 3, 13.0))?;
        assert_eq!(
            second
                .network
                .as_ref()
                .and_then(|v| v.first())
                .and_then(|v| v.received_bytes_per_second),
            Some(100.0)
        );
        assert_eq!(
            second
                .disks
                .as_ref()
                .and_then(|v| v.first())
                .and_then(|v| v.written_bytes_per_second),
            Some(300.0)
        );
        let reset = monitor.update(io(10, 4, 14.0))?;
        assert_eq!(
            reset
                .network
                .as_ref()
                .and_then(|v| v.first())
                .and_then(|v| v.received_bytes_per_second),
            None
        );
        let mut renamed = io(200, 5, 15.0);
        if let Some(values) = renamed.network.as_mut() {
            values[0].id = "eth1".into();
        }
        assert_eq!(
            monitor
                .update(renamed)?
                .network
                .as_ref()
                .and_then(|v| v.first())
                .and_then(|v| v.received_bytes_per_second),
            None
        );
        let mut reboot = io(300, 6, 1.0);
        reboot.boot_id = "newboot".into();
        assert_eq!(
            monitor
                .update(reboot)?
                .disks
                .as_ref()
                .and_then(|v| v.first())
                .and_then(|v| v.read_bytes_per_second),
            None
        );
        Ok(())
    }
    #[test]
    fn process_interval_cpu_is_whole_machine_share_and_reused_pid_is_unknown() -> Result<()> {
        use super::super::{ProcessCpu, ProcessSample};
        let mut monitor = ResourceMonitor::default();
        let processes = |total, cpu, start_id: &str| {
            let mut sample = sample(total, 0, total as f64);
            sample.processes = Some(ProcessesSample {
                total_cpu: Some(total),
                total_count: 1,
                truncated: false,
                entries: vec![ProcessSample {
                    pid: 42,
                    start_id: start_id.into(),
                    name: "worker".into(),
                    user: None,
                    memory_bytes: Some(4096),
                    cpu: ProcessCpu::Counter(cpu),
                }],
            });
            sample
        };
        assert_eq!(
            monitor
                .update(processes(100, 20, "old"))?
                .processes
                .and_then(|v| v.first().and_then(|v| v.cpu_percent)),
            None
        );
        assert_eq!(
            monitor
                .update(processes(300, 70, "old"))?
                .processes
                .and_then(|v| v.first().and_then(|v| v.cpu_percent)),
            Some(25.0)
        );
        assert_eq!(
            monitor
                .update(processes(500, 170, "new"))?
                .processes
                .and_then(|v| v.first().and_then(|v| v.cpu_percent)),
            None
        );
        assert_eq!(
            monitor
                .update(processes(700, 10, "new"))?
                .processes
                .and_then(|v| v.first().and_then(|v| v.cpu_percent)),
            None
        );
        Ok(())
    }
    #[test]
    fn timed_process_cpu_uses_fractional_monotonic_interval_and_resets_identity() -> Result<()> {
        use super::super::{ProcessCpu, ProcessSample};
        let mut monitor = ResourceMonitor::default();
        let start = Instant::now();
        let timed = |ticks, millis, identity: &str, boot: &str| {
            let mut sample = sample(100 + millis, 0, millis as f64 / 1000.0);
            sample.logical_cpus = 2;
            if let CpuSample::Counters { aggregate, cores } = &mut sample.cpu {
                cores.push(CpuCoreCounter {
                    name: "1".into(),
                    counter: *aggregate,
                });
            }
            sample.boot_id = boot.into();
            sample.captured_at = start + std::time::Duration::from_millis(millis);
            sample.processes = Some(ProcessesSample {
                total_cpu: None,
                total_count: 1,
                truncated: false,
                entries: vec![ProcessSample {
                    pid: 42,
                    start_id: identity.into(),
                    name: "timed".into(),
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
        let cpu = |usage: ResourceUsage| {
            usage
                .processes
                .and_then(|values| values.first().and_then(|value| value.cpu_percent))
        };
        assert_eq!(cpu(monitor.update(timed(100, 0, "first", "boot"))?), None);
        // 37 centiseconds in 0.74 seconds = half one CPU, one quarter of two CPUs.
        assert_eq!(
            cpu(monitor.update(timed(137, 740, "first", "boot"))?),
            Some(25.0)
        );
        assert_eq!(
            cpu(monitor.update(timed(200, 1000, "replacement", "boot"))?),
            None
        );
        assert_eq!(
            cpu(monitor.update(timed(225, 1500, "replacement", "newboot"))?),
            None
        );
        assert_eq!(
            cpu(monitor.update(timed(10, 2000, "replacement", "newboot"))?),
            None
        );
        assert_eq!(
            cpu(monitor.update(timed(20, 2000, "replacement", "newboot"))?),
            None
        );
        Ok(())
    }
}
