//! Windows CIM data through the existing encoded PowerShell local/SSH transport.
use super::{CpuCoreCounter, CpuCounter, CpuSample, MemorySample, ResourceSample, SwapSample};
use crate::{machine::Machine, powershell};
use anyhow::{Context as _, Result, ensure};
use serde::Deserialize;

const SCRIPT: &str = r#"
$os = Get-CimInstance -ClassName Win32_OperatingSystem
$cpu = @(Get-CimInstance -ClassName Win32_PerfRawData_PerfOS_Processor | ForEach-Object {
    [pscustomobject]@{ name = [string]$_.Name; total = [uint64]$_.Timestamp_Sys100NS; idle = [uint64]$_.PercentProcessorTime }
})
$swap = $null
try {
    $files = @(Get-CimInstance -ClassName Win32_PageFileUsage -ErrorAction Stop)
    [uint64]$total = 0; [uint64]$used = 0
    foreach ($file in $files) { $total += [uint64]$file.AllocatedBaseSize * 1048576; $used += [uint64]$file.CurrentUsage * 1048576 }
    $swap = [pscustomobject]@{ total = $total; free = [math]::Max(0, [long]$total - [long]$used) }
} catch { }
[pscustomobject]@{
    cpu = $cpu
    memory_total = [uint64]$os.TotalVisibleMemorySize * 1024
    memory_available = [uint64]$os.FreePhysicalMemory * 1024
    uptime = [math]::Max(0, ((Get-Date) - $os.LastBootUpTime).TotalSeconds)
    boot = $os.LastBootUpTime.ToUniversalTime().ToString('o')
    swap = $swap
} | ConvertTo-Json -Depth 4 -Compress
"#;

pub(super) async fn sample(machine: &mut Machine) -> Result<ResourceSample> {
    let output = powershell::run(machine, SCRIPT).await?;
    parse(&output)
}

// WMI uint64 values may be represented as decimal strings by older tools.
#[derive(Deserialize)]
#[serde(untagged)]
enum Number {
    Integer(u64),
    Text(String),
}
impl Number {
    fn value(self) -> Result<u64> {
        match self {
            Self::Integer(value) => Ok(value),
            Self::Text(value) => value.parse().context("Invalid Windows resource counter"),
        }
    }
}
#[derive(Deserialize)]
struct Counter {
    name: String,
    total: Number,
    idle: Number,
}
#[derive(Deserialize)]
struct Swap {
    total: Number,
    free: Number,
}
#[derive(Deserialize)]
struct Snapshot {
    cpu: Vec<Counter>,
    memory_total: Number,
    memory_available: Number,
    uptime: f64,
    boot: String,
    swap: Option<Swap>,
}

fn parse(output: &str) -> Result<ResourceSample> {
    let snapshot: Snapshot = serde_json::from_str(output.trim_start_matches('\u{feff}'))
        .context("Invalid Windows resource response")?;
    let mut aggregate = None;
    let mut cores = Vec::new();
    for cpu in snapshot.cpu {
        let counter = CpuCounter {
            total: cpu.total.value()?,
            idle: cpu.idle.value()?,
        };
        // PercentProcessorTime is an inverse 100ns timer, not a percentage:
        // usage = 100 * (1 - delta(counter) / delta(Timestamp_Sys100NS)).
        if cpu.name == "_Total" {
            ensure!(
                aggregate.is_none(),
                "Duplicate Windows aggregate CPU counter"
            );
            aggregate = Some(counter);
        } else {
            cores.push(CpuCoreCounter {
                name: cpu.name,
                counter,
            });
        }
    }
    cores.sort_by(|first, second| first.name.cmp(&second.name));
    let logical_cpus = cores.len();
    let swap = snapshot
        .swap
        .map(|swap| {
            Ok::<_, anyhow::Error>(SwapSample {
                total_bytes: swap.total.value()?,
                free_bytes: swap.free.value()?,
            })
        })
        .transpose()?;
    let sample = ResourceSample {
        cpu: CpuSample::Counters {
            aggregate: aggregate.context("Missing Windows aggregate CPU counter")?,
            cores,
        },
        logical_cpus,
        memory: MemorySample {
            total_bytes: snapshot.memory_total.value()?,
            available_bytes: snapshot.memory_available.value()?,
            estimated: false,
        },
        swap,
        load_average: None,
        uptime_seconds: snapshot.uptime,
        boot_id: snapshot.boot,
    };
    sample.validate()?;
    Ok(sample)
}

#[cfg(test)]
mod tests {
    use super::super::ResourceMonitor;
    use super::*;
    #[test]
    fn raw_inverse_timer_becomes_interval_cpu_and_numeric_strings_work() -> Result<()> {
        let mut monitor = ResourceMonitor::default();
        let first = r#"{"cpu":[{"name":"_Total","total":"1000","idle":"400"},{"name":"0","total":1000,"idle":400}],"memory_total":"8192","memory_available":2048,"uptime":10.0,"boot":"2026-01-01","swap":{"total":4096,"free":1024}}"#;
        let second = first
            .replace("1000", "2000")
            .replace("400", "600")
            .replace("10.0", "11.0");
        assert_eq!(monitor.update(parse(first)?)?.cpu_percent, None);
        let usage = monitor.update(parse(&second)?)?;
        assert_eq!(usage.cpu_percent, Some(80.0));
        assert_eq!(usage.cores[0].percent, Some(80.0));
        assert_eq!(usage.memory.used_percent(), 75.0);
        assert!(usage.load_average.is_none());
        assert_eq!(usage.swap.context("Expected pagefile")?.used_bytes(), 3072);
        Ok(())
    }
    #[test]
    fn unavailable_or_invalid_windows_counters_are_errors() {
        assert!(parse(r#"{"cpu":[],"memory_total":8192,"memory_available":2048,"uptime":0,"boot":"boot","swap":null}"#).is_err());
        assert!(parse(r#"{"cpu":[{"name":"_Total","total":"not-a-counter","idle":0}],"memory_total":8192,"memory_available":2048,"uptime":0,"boot":"boot","swap":null}"#).is_err());
        assert!(parse("not json").is_err());
    }
}
