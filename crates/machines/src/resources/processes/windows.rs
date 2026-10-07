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

pub(crate) async fn sample(
    machine: &mut Machine,
    uptime_seconds: f64,
    logical_cpus: usize,
) -> Result<ProcessesSample> {
    parse(
        &powershell::run(machine, SCRIPT).await?,
        uptime_seconds,
        logical_cpus,
    )
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
fn parse(output: &str, uptime_seconds: f64, logical_cpus: usize) -> Result<ProcessesSample> {
    ensure!(
        uptime_seconds.is_finite() && uptime_seconds >= 0.0 && logical_cpus > 0,
        "Invalid Windows process clock"
    );
    let clock = uptime_seconds * 10_000_000.0 * logical_cpus as f64;
    ensure!(clock < u64::MAX as f64, "Windows process clock overflow");
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
            .map_or(ProcessCpu::Unknown, ProcessCpu::Counter);
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
        total_cpu: Some(clock.round() as u64),
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
            10.0,
            4,
        )?;
        assert_eq!(sample.total_cpu, Some(400_000_000));
        assert_eq!(sample.total_count, 4);
        assert!(sample.truncated);
        assert_eq!(sample.entries.len(), 2);
        assert_eq!(sample.entries[0].memory_bytes, Some(9_007_199_254_740_993));
        assert!(matches!(sample.entries[1].cpu, ProcessCpu::Unknown));
        assert!(parse("{}", 10.0, 4).is_err());
        Ok(())
    }
}
