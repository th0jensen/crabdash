//! Cumulative 100ns process time; creation timestamps guard against PID reuse.
use super::super::{ProcessCpu, ProcessSample, ProcessesSample};
use crate::{machine::Machine, powershell};
use anyhow::{Context as _, Result, ensure};
use serde::Deserialize;

const SCRIPT: &str = r#"
$all = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop)
$rows = @($all | Select-Object -First 8192 | ForEach-Object {
    $cpu = $null; $start = ''
    if ($null -ne $_.KernelModeTime -and $null -ne $_.UserModeTime) { $cpu = ([uint64]$_.KernelModeTime + [uint64]$_.UserModeTime).ToString() }
    if ($null -ne $_.CreationDate) { $start = $_.CreationDate.ToUniversalTime().ToString('o') }
    $memory = $null
    if ($null -ne $_.WorkingSetSize) { $memory = ([uint64]$_.WorkingSetSize).ToString() }
    [pscustomobject]@{ pid = [uint32]$_.ProcessId; name = [string]$_.Name; start = $start; cpu = $cpu; memory = $memory }
})
[pscustomobject]@{ count = $all.Count; rows = $rows } | ConvertTo-Json -Depth 4 -Compress
"#;

pub(crate) async fn sample(machine: &mut Machine) -> Result<ProcessesSample> {
    parse(&powershell::run(machine, SCRIPT).await?)
}
#[derive(Deserialize)]
struct Row {
    pid: u32,
    name: String,
    start: String,
    cpu: Option<String>,
    memory: Option<String>,
}
#[derive(Deserialize)]
struct Snapshot {
    count: usize,
    rows: Vec<Row>,
}
fn parse(output: &str) -> Result<ProcessesSample> {
    let snapshot: Snapshot = serde_json::from_str(output.trim_start_matches('\u{feff}'))
        .context("Invalid Windows process response")?;
    ensure!(
        snapshot.count >= snapshot.rows.len(),
        "Invalid Windows process count"
    );
    let mut entries = Vec::new();
    let truncated = snapshot.count > snapshot.rows.len();
    for row in snapshot.rows {
        // PID 0 represents CPU idle time, not a workload. Protected processes
        // without creation data cannot safely share a subsequent PID baseline.
        if row.pid == 0 || row.start.is_empty() {
            continue;
        }
        let cpu = row
            .cpu
            .map(|value| {
                value
                    .parse::<u64>()
                    .context("Invalid Windows process CPU time")
            })
            .transpose()?
            .map_or(ProcessCpu::Unknown, |ticks| ProcessCpu::TimedCounter {
                ticks,
                ticks_per_second: 10_000_000,
            });
        let memory_bytes = row
            .memory
            .map(|value| {
                value
                    .parse::<u64>()
                    .context("Invalid Windows process memory")
            })
            .transpose()?;
        entries.push(ProcessSample {
            pid: row.pid,
            start_id: row.start,
            name: row.name,
            user: None,
            memory_bytes,
            cpu,
        });
    }
    Ok(ProcessesSample {
        total_cpu: None,
        entries,
        total_count: snapshot.count,
        truncated,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_creation_identity_unknown_counters_and_total_count() -> Result<()> {
        let sample = parse(
            r#"{"count":4,"rows":[{"pid":0,"name":"Idle","start":"boot","cpu":"999","memory":"0"},{"pid":12,"name":"app.exe","start":"2026-10-07T12:00:00Z","cpu":"123456789","memory":"9007199254740993"},{"pid":13,"name":"protected.exe","start":"start","cpu":null,"memory":null}]}"#,
        )?;
        assert_eq!(sample.total_cpu, None);
        assert!(matches!(
            sample.entries[0].cpu,
            ProcessCpu::TimedCounter {
                ticks: 123456789,
                ticks_per_second: 10_000_000
            }
        ));
        assert_eq!(sample.total_count, 4);
        assert!(sample.truncated);
        assert_eq!(sample.entries.len(), 2);
        assert_eq!(sample.entries[0].memory_bytes, Some(9_007_199_254_740_993));
        assert!(matches!(sample.entries[1].cpu, ProcessCpu::Unknown));
        assert!(parse("{}").is_err());
        Ok(())
    }
    #[test]
    fn parsed_100ns_time_uses_monotonic_interval_and_resets_pid_or_boot_identity() -> Result<()> {
        use super::super::super::{CpuSample, MemorySample, ResourceMonitor, ResourceSample};
        use std::time::{Duration, Instant};
        let started = Instant::now();
        let snapshot = |ticks: u64,
                        elapsed_millis: u64,
                        identity: &str,
                        boot: &str|
         -> Result<ResourceSample> {
            let output=serde_json::json!({"count":1,"rows":[{"pid":42,"name":"app.exe","start":identity,"cpu":ticks.to_string(),"memory":"4096"}]}).to_string();
            Ok(ResourceSample {
                cpu: CpuSample::Sampled { percent: 10.0 },
                logical_cpus: 4,
                memory: MemorySample {
                    total_bytes: 4096,
                    available_bytes: 1024,
                    estimated: false,
                },
                swap: None,
                load_average: None,
                // Holding target uptime constant proves the process denominator
                // comes from monotonic capture time, not a rounded wall clock.
                uptime_seconds: 100.0,
                boot_id: boot.into(),
                captured_at: started + Duration::from_millis(elapsed_millis),
                processes: Some(parse(&output)?),
                network: None,
                disks: None,
                gpus: None,
            })
        };
        let mut monitor = ResourceMonitor::default();
        let percent = |usage: super::super::super::ResourceUsage| {
            usage
                .processes
                .and_then(|values| values.first().and_then(|value| value.cpu_percent))
        };
        assert_eq!(
            percent(monitor.update(snapshot(100_000_000, 0, "start", "boot")?)?),
            None
        );
        // 1.25 seconds of CPU over 1.25 elapsed seconds on four logical CPUs.
        assert_eq!(
            percent(monitor.update(snapshot(112_500_000, 1250, "start", "boot")?)?),
            Some(25.0)
        );
        assert_eq!(
            percent(monitor.update(snapshot(125_000_000, 2500, "reused", "boot")?)?),
            None
        );
        assert_eq!(
            percent(monitor.update(snapshot(137_500_000, 3750, "reused", "newboot")?)?),
            None
        );
        assert_eq!(
            percent(monitor.update(snapshot(10, 5000, "reused", "newboot")?)?),
            None
        );
        Ok(())
    }
}
