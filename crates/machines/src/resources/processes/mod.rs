//! Process identities and interval CPU shares; PID reuse never shares a baseline.
pub(super) mod linux;
pub(super) mod macos;
pub(super) mod windows;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum ProcessCpu {
    Unknown,
    Counter(u64),
    /// Cumulative CPU time without target-side aggregate counters (BSD ps TIME).
    TimedCounter {
        ticks: u64,
        ticks_per_second: u64,
    },
    Percent(f64),
}
#[derive(Clone, Debug)]
pub struct ProcessSample {
    pub pid: u32,
    pub start_id: String,
    pub name: String,
    pub user: Option<String>,
    pub memory_bytes: Option<u64>,
    pub cpu: ProcessCpu,
}
#[derive(Clone, Debug)]
pub struct ProcessesSample {
    pub total_cpu: Option<u64>,
    pub entries: Vec<ProcessSample>,
    pub total_count: usize,
    pub truncated: bool,
}
#[derive(Clone, Debug)]
pub struct ProcessUsage {
    pub pid: u32,
    pub start_id: String,
    pub name: String,
    pub user: Option<String>,
    pub memory_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
}
pub(super) fn usage(
    sample: &ProcessesSample,
    previous: Option<&ProcessesSample>,
    elapsed_seconds: Option<f64>,
    logical_cpus: usize,
) -> Vec<ProcessUsage> {
    let old: HashMap<_, _> = previous
        .into_iter()
        .flat_map(|p| &p.entries)
        .map(|p| ((p.pid, p.start_id.as_str()), p))
        .collect();
    let denominator = sample
        .total_cpu
        .zip(previous.and_then(|p| p.total_cpu))
        .and_then(|(new, old)| new.checked_sub(old))
        .filter(|value| *value > 0);
    sample
        .entries
        .iter()
        .filter(|p| !p.start_id.is_empty())
        .map(|p| {
            let percent = match p.cpu {
                ProcessCpu::Percent(percent)
                    if percent.is_finite() && (0.0..=100.0).contains(&percent) =>
                {
                    Some(percent)
                }
                ProcessCpu::Counter(new) => {
                    old.get(&(p.pid, p.start_id.as_str()))
                        .and_then(|old| match old.cpu {
                            ProcessCpu::Counter(old) => {
                                new.checked_sub(old).zip(denominator).map(|(delta, total)| {
                                    (delta as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
                                })
                            }
                            _ => None,
                        })
                }
                ProcessCpu::TimedCounter {
                    ticks,
                    ticks_per_second,
                } if ticks_per_second > 0 && logical_cpus > 0 => old
                    .get(&(p.pid, p.start_id.as_str()))
                    .and_then(|old| match old.cpu {
                        ProcessCpu::TimedCounter {
                            ticks: old_ticks,
                            ticks_per_second: old_frequency,
                        } if old_frequency == ticks_per_second => {
                            let delta = ticks.checked_sub(old_ticks)?;
                            let elapsed = elapsed_seconds
                                .filter(|value| value.is_finite() && *value > 0.0)?;
                            Some(
                                (delta as f64
                                    / (elapsed * ticks_per_second as f64 * logical_cpus as f64)
                                    * 100.0)
                                    .clamp(0.0, 100.0),
                            )
                        }
                        _ => None,
                    }),
                _ => None,
            };
            ProcessUsage {
                pid: p.pid,
                start_id: p.start_id.clone(),
                name: p.name.clone(),
                user: p.user.clone(),
                memory_bytes: p.memory_bytes,
                cpu_percent: percent,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_snapshot_retains_idle_processes_and_safe_cpu_baselines() {
        let previous_entries: Vec<_> = (0..150)
            .map(|pid| ProcessSample {
                pid,
                start_id: format!("start-{pid}"),
                name: format!("process-{pid}"),
                user: None,
                memory_bytes: Some(pid as u64),
                cpu: ProcessCpu::Counter(200),
            })
            .collect();
        let mut entries = previous_entries.clone();
        for entry in &mut entries {
            entry.cpu = ProcessCpu::Counter(300);
        }
        entries[100].start_id = "reused-pid".into();
        entries[101].cpu = ProcessCpu::Unknown;
        entries[101].memory_bytes = None;
        entries[149].name = "idle-target".into();
        entries[149].cpu = ProcessCpu::Counter(200);
        let previous = ProcessesSample {
            entries: previous_entries,
            total_cpu: Some(1000),
            total_count: 150,
            truncated: false,
        };
        let sample = ProcessesSample {
            entries,
            total_cpu: Some(2000),
            total_count: 150,
            truncated: false,
        };
        let results = usage(&sample, Some(&previous), Some(1.0), 1);
        assert_eq!(results.len(), 150);
        assert_eq!(
            results
                .iter()
                .map(|process| process.pid)
                .collect::<Vec<_>>(),
            (0..150).collect::<Vec<_>>()
        );
        assert_eq!(results[0].cpu_percent, Some(10.0));
        assert_eq!(results[100].start_id, "reused-pid");
        assert_eq!(results[100].cpu_percent, None);
        assert_eq!(results[101].cpu_percent, None);
        assert_eq!(results[101].memory_bytes, None);
        assert_eq!(results[149].name, "idle-target");
        assert_eq!(results[149].cpu_percent, Some(0.0));
        assert!(
            usage(&sample, None, None, 1)
                .iter()
                .all(|process| process.cpu_percent.is_none())
        );
    }
}
