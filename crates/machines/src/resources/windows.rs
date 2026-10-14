//! Windows CIM data through the existing encoded PowerShell local/SSH transport.
use super::collector::ResourceCollector;
use super::{CpuCoreCounter, CpuCounter, CpuSample, MemorySample, ResourceSample, SwapSample};
use anyhow::{Context as _, Result, ensure};
use serde::Deserialize;

// PowerShell casts $null to numeric zero. Check required CIM properties before
// casting so an absent idle/free-memory counter cannot become a busy/full reading.
const SCRIPT: &str = r#"
$os = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop
if ($null -eq $os -or $null -eq $os.TotalVisibleMemorySize -or $null -eq $os.FreePhysicalMemory -or $null -eq $os.LastBootUpTime) {
    throw 'Required operating-system resource fields unavailable'
}
$cpu = @(Get-CimInstance -ClassName Win32_PerfRawData_PerfOS_Processor -ErrorAction Stop | ForEach-Object {
    if ($null -eq $_.Name -or $null -eq $_.Timestamp_Sys100NS -or $null -eq $_.PercentProcessorTime) {
        throw 'Required CPU resource fields unavailable'
    }
    [pscustomobject]@{ name = [string]$_.Name; total = [uint64]$_.Timestamp_Sys100NS; idle = [uint64]$_.PercentProcessorTime }
})
$swap = $null
try {
    $files = @(Get-CimInstance -ClassName Win32_PageFileUsage -ErrorAction Stop)
    [uint64]$total = 0; [uint64]$used = 0
    foreach ($file in $files) {
        if ($null -eq $file.AllocatedBaseSize -or $null -eq $file.CurrentUsage) { throw 'Pagefile counters unavailable' }
        $total += [uint64]$file.AllocatedBaseSize * 1048576; $used += [uint64]$file.CurrentUsage * 1048576
    }
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

pub(super) async fn sample(
    machine: &mut ResourceCollector<'_>,
    on_core: &mut impl FnMut(ResourceSample),
) -> Result<ResourceSample> {
    let captured_at = std::time::Instant::now();
    let output = machine.powershell(SCRIPT).await?;
    let mut sample = parse(&output)?;
    sample.captured_at = captured_at;
    machine.check_budget()?;
    on_core(sample.clone());
    if let Ok(output) = machine.powershell(&optional_script()).await {
        enrich(&mut sample, &output);
    }
    Ok(sample)
}

/// One optional query process avoids repeated PowerShell startup, CIM setup and
/// compiler allocations. Other inventories run during the GPU counter interval.
/// Every domain catches its own failure; absent counters remain unknown.
fn optional_script() -> String {
    let gpu_first = super::gpu::windows::script(false);
    let gpu_second = super::gpu::windows::script(true);
    let processes = super::processes::windows::script();
    let network = super::network::windows::SCRIPT;
    let disks = super::disks::windows::SCRIPT;
    format!(
        r#"
$crabdash_optional = [ordered]@{{ processes = $null; network = $null; disks = $null; gpu_first = $null; gpu_second = $null }}
try {{ $crabdash_optional.gpu_first = & {{ {gpu_first} }} }} catch {{}}
$crabdash_gpu_clock = [System.Diagnostics.Stopwatch]::StartNew()
try {{ $crabdash_optional.processes = & {{ {processes} }} }} catch {{}}
try {{ $crabdash_optional.network = & {{ {network} }} }} catch {{}}
try {{ $crabdash_optional.disks = & {{ {disks} }} }} catch {{}}
if ($null -ne $crabdash_optional.gpu_first) {{
    try {{
        $crabdash_gpu_supported = ($crabdash_optional.gpu_first | ConvertFrom-Json -ErrorAction Stop).supported
        $crabdash_gpu_wait = 1000 - $crabdash_gpu_clock.ElapsedMilliseconds
        if ($crabdash_gpu_supported -and $crabdash_gpu_wait -gt 0) {{ Start-Sleep -Milliseconds $crabdash_gpu_wait }}
    }} catch {{}}
}}
try {{ $crabdash_optional.gpu_second = & {{ {gpu_second} }} }} catch {{}}
# One JSON envelope avoids interleaving Console output and PowerShell's output
# pipeline, and works with Windows CRLF without custom line framing.
[pscustomobject]$crabdash_optional | ConvertTo-Json -Depth 3 -Compress
"#
    )
}

#[derive(Deserialize)]
struct OptionalSnapshot {
    processes: Option<String>,
    network: Option<String>,
    disks: Option<String>,
    gpu_first: Option<String>,
    gpu_second: Option<String>,
}

fn enrich(sample: &mut ResourceSample, output: &str) {
    let Ok(snapshot) =
        serde_json::from_str::<OptionalSnapshot>(output.trim_start_matches('\u{feff}'))
    else {
        return;
    };
    sample.processes = snapshot
        .processes
        .as_deref()
        .and_then(|output| super::processes::windows::parse(output).ok());
    sample.network = snapshot
        .network
        .as_deref()
        .and_then(|output| super::network::windows::parse(output).ok());
    sample.disks = snapshot
        .disks
        .as_deref()
        .and_then(|output| super::disks::windows::parse(output).ok());
    sample.gpus = snapshot
        .gpu_first
        .as_deref()
        .zip(snapshot.gpu_second.as_deref())
        .and_then(|(first, second)| super::gpu::windows::parse_interval(first, second).ok());
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
        captured_at: std::time::Instant::now(),
        processes: None,
        network: None,
        disks: None,
        gpus: None,
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
    fn zero_snapshot() -> serde_json::Value {
        serde_json::json!({
            "cpu": [
                {"name": "_Total", "total": 0, "idle": 0},
                {"name": "0", "total": 0, "idle": 0}
            ],
            "memory_total": 8192,
            "memory_available": 0,
            "uptime": 0,
            "boot": "boot",
            "swap": {"total": 0, "free": 0}
        })
    }

    #[test]
    fn missing_or_null_required_fields_are_not_zero_readings() -> Result<()> {
        for field in ["cpu", "memory_total", "memory_available", "uptime", "boot"] {
            let mut response = zero_snapshot();
            response[field] = serde_json::Value::Null;
            assert!(parse(&response.to_string()).is_err(), "null {field}");
            response
                .as_object_mut()
                .context("Expected fixture object")?
                .remove(field);
            assert!(parse(&response.to_string()).is_err(), "missing {field}");
        }
        for index in 0..2 {
            for field in ["name", "total", "idle"] {
                let mut response = zero_snapshot();
                response["cpu"][index][field] = serde_json::Value::Null;
                assert!(
                    parse(&response.to_string()).is_err(),
                    "null CPU {index} {field}"
                );
                response["cpu"][index]
                    .as_object_mut()
                    .context("Expected fixture CPU object")?
                    .remove(field);
                assert!(
                    parse(&response.to_string()).is_err(),
                    "missing CPU {index} {field}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn genuine_zero_idle_available_memory_and_pagefile_values_remain_valid() -> Result<()> {
        let mut response = zero_snapshot();
        let mut monitor = ResourceMonitor::default();
        let first = monitor.update(parse(&response.to_string())?)?;
        assert_eq!(first.cpu_percent, None);
        assert_eq!(first.memory.available_bytes, 0);
        assert_eq!(first.memory.used_percent(), 100.0);
        assert_eq!(
            first
                .swap
                .context("Expected zero-size pagefile")?
                .total_bytes,
            0
        );
        for index in 0..2 {
            response["cpu"][index]["total"] = 100.into();
        }
        response["uptime"] = 1.into();
        let busy = monitor.update(parse(&response.to_string())?)?;
        assert_eq!(busy.cpu_percent, Some(100.0));
        assert_eq!(busy.cores[0].percent, Some(100.0));
        for index in 0..2 {
            response["cpu"][index]["total"] = 200.into();
            response["cpu"][index]["idle"] = 100.into();
        }
        response["uptime"] = 2.into();
        response["memory_available"] = 8192.into();
        response["swap"] = serde_json::Value::Null;
        let idle = monitor.update(parse(&response.to_string())?)?;
        assert_eq!(idle.cpu_percent, Some(0.0));
        assert_eq!(idle.cores[0].percent, Some(0.0));
        assert_eq!(idle.memory.used_percent(), 0.0);
        assert!(idle.swap.is_none());
        Ok(())
    }

    #[test]
    fn optional_envelope_handles_crlf_unicode_and_independent_failures() -> Result<()> {
        let mut sample = parse(&zero_snapshot().to_string())?;
        let envelope = serde_json::json!({
            "processes": "unavailable",
            "network": r#"[{"id":"Ethernet 雪","received":"1234","sent":"5678"}]"#,
            "disks": null,
            "gpu_first": r#"{"supported":false,"inventory_available":false,"engines":[],"memory":[],"inventory":[]}"#,
            "gpu_second": r#"{"supported":false,"inventory_available":true,"engines":[],"memory":[],"inventory":[{"id":"PCI\\GPU","name":"GPU","vendor":"Virtio","driver":"1"}]}"#,
        });
        enrich(&mut sample, &format!("\u{feff}{}\r\n", envelope));
        assert!(sample.processes.is_none());
        assert!(sample.disks.is_none());
        let network = sample
            .network
            .as_ref()
            .context("Expected valid network despite process failure")?;
        assert_eq!(network[0].id, "Ethernet 雪");
        assert_eq!(network[0].received_bytes, 1234);
        let gpus = sample
            .gpus
            .as_ref()
            .context("Expected GPU inventory without utilization counters")?;
        assert_eq!(gpus[0].name, "GPU");
        assert_eq!(gpus[0].busy_percent, None);
        // A malformed optional envelope cannot invalidate validated core data.
        enrich(&mut sample, "broken optional response");
        sample.validate()?;
        assert_eq!(sample.memory.total_bytes, 8192);
        Ok(())
    }

    #[test]
    fn unavailable_or_invalid_windows_counters_are_errors() {
        assert!(parse(r#"{"cpu":[],"memory_total":8192,"memory_available":2048,"uptime":0,"boot":"boot","swap":null}"#).is_err());
        assert!(parse(r#"{"cpu":[{"name":"_Total","total":"not-a-counter","idle":0}],"memory_total":8192,"memory_available":2048,"uptime":0,"boot":"boot","swap":null}"#).is_err());
        assert!(parse("not json").is_err());
    }
}
