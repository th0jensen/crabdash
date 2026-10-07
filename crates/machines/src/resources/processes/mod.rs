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
    let mut values: Vec<_> = sample
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
        .collect();
    values.sort_by(|a, b| {
        b.cpu_percent
            .unwrap_or(-1.0)
            .total_cmp(&a.cpu_percent.unwrap_or(-1.0))
            .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
            .then_with(|| a.pid.cmp(&b.pid))
    });
    values.truncate(100);
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_results_are_bounded_after_ranking_not_before() {
        let entries = (0..150)
            .map(|pid| ProcessSample {
                pid,
                start_id: pid.to_string(),
                name: "process".into(),
                user: None,
                memory_bytes: Some(pid as u64),
                cpu: ProcessCpu::Percent(pid as f64 / 2.0),
            })
            .collect();
        let sample = ProcessesSample {
            entries,
            total_cpu: None,
            total_count: 150,
            truncated: false,
        };
        let usage = usage(&sample, None, None, 1);
        assert_eq!(usage.len(), 100);
        assert_eq!(usage[0].pid, 149);
        assert_eq!(usage[99].pid, 50);
    }
}
