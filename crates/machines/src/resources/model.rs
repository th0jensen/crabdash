//! Platform-neutral samples and interval CPU calculations. Live data is never persisted.
use anyhow::{Result, ensure};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuCounter {
    pub total: u64,
    pub idle: u64,
}

impl CpuCounter {
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
                    } if cores
                        .iter()
                        .zip(old_cores)
                        .all(|(new, old)| new.name == old.name) =>
                    {
                        Some((*old, old_cores))
                    }
                    _ => None,
                });
                let cpu = previous_counters.and_then(|(old, _)| aggregate.usage_since(old));
                // A reset of the aggregate invalidates every per-core interval,
                // even when an individual core's counter happens to increase.
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
        let usage = ResourceUsage {
            cpu_percent,
            cores,
            logical_cpus: sample.logical_cpus,
            memory: sample.memory,
            swap: sample.swap,
            load_average: sample.load_average,
            uptime_seconds: sample.uptime_seconds,
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
}
