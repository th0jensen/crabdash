//! WDDM engine timers grouped by exact LUID/physical-adapter identity.
//! Different engines can execute concurrently: overall activity is the busiest
//! engine, not the sum of the 3D/copy/video percentages.
use super::super::GpuSample;
use crate::{machine::Machine, powershell};
use anyhow::{Context as _, Result, ensure};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

const SCRIPT: &str = r#"
$engines = @(); $memory = @(); $inventory = @(); $supported = $false; $inventory_available = $false
try {
    $class = Get-CimClass -ClassName Win32_PerfRawData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop
    $type = $class.CimClassProperties['UtilizationPercentage'].Qualifiers['CounterType'].Value
    if ([uint64]$type -eq 542180608) {
        $engines = @(Get-CimInstance -ClassName Win32_PerfRawData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop | ForEach-Object {
            if ($null -eq $_.UtilizationPercentage -or $null -eq $_.Timestamp_Sys100NS) { throw 'GPU engine counters unavailable' }
            [pscustomobject]@{ id = [string]$_.Name; ticks = ([uint64]$_.UtilizationPercentage).ToString(); clock = ([uint64]$_.Timestamp_Sys100NS).ToString() }
        })
        $supported = $true
    }
} catch { }
try {
    $memory = @(Get-CimInstance -ClassName Win32_PerfRawData_GPUPerformanceCounters_GPUAdapterMemory -ErrorAction Stop | ForEach-Object {
        if ($null -eq $_.DedicatedUsage) { throw 'GPU memory counters unavailable' }
        [pscustomobject]@{ id = [string]$_.Name; used = ([uint64]$_.DedicatedUsage).ToString() }
    })
} catch { }
try {
    $inventory = @(Get-CimInstance -ClassName Win32_VideoController -ErrorAction Stop | ForEach-Object {
        [pscustomobject]@{ id = [string]$_.PNPDeviceID; name = [string]$_.Name; vendor = [string]$_.AdapterCompatibility; driver = [string]$_.DriverVersion }
    })
    $inventory_available = $true
} catch { }
[pscustomobject]@{ supported = $supported; inventory_available = $inventory_available; engines = $engines; memory = $memory; inventory = $inventory } | ConvertTo-Json -Depth 4 -Compress
"#;

pub(crate) async fn sample(machine: &mut Machine) -> Result<Vec<GpuSample>> {
    let first = parse(&powershell::run(machine, SCRIPT).await?)?;
    if !first.supported {
        ensure!(
            first.inventory_available,
            "GPU telemetry provider unavailable"
        );
        return Ok(inventory(first.inventory));
    }
    smol::Timer::after(Duration::from_secs(1)).await;
    let second = parse(&powershell::run(machine, SCRIPT).await?)?;
    if !second.supported {
        ensure!(
            second.inventory_available,
            "GPU telemetry provider unavailable"
        );
        return Ok(inventory(second.inventory));
    }
    cook(&first, &second)
}
#[derive(Deserialize)]
struct Engine {
    id: String,
    ticks: String,
    clock: String,
}
#[derive(Deserialize)]
struct Memory {
    id: String,
    used: String,
}
#[derive(Deserialize)]
struct Adapter {
    id: String,
    name: String,
    vendor: String,
    driver: String,
}
#[derive(Deserialize)]
struct Snapshot {
    supported: bool,
    inventory_available: bool,
    engines: Vec<Engine>,
    memory: Vec<Memory>,
    inventory: Vec<Adapter>,
}
fn parse(output: &str) -> Result<Snapshot> {
    serde_json::from_str(output.trim_start_matches('\u{feff}'))
        .context("Invalid Windows GPU response")
}
// Never join Win32_VideoController to performance instances by enumeration order.
// The provider does not expose that relation; PNP inventory remains a fallback.
fn inventory(adapters: Vec<Adapter>) -> Vec<GpuSample> {
    adapters
        .into_iter()
        .filter(|adapter| !adapter.id.is_empty())
        .map(|adapter| GpuSample {
            id: adapter.id,
            name: adapter.name,
            vendor: adapter.vendor,
            driver: (!adapter.driver.is_empty()).then_some(adapter.driver),
            busy_percent: None,
            memory_used_bytes: None,
            memory_total_bytes: None,
            temperature_celsius: None,
        })
        .collect()
}
fn adapter_id(instance: &str) -> Option<&str> {
    let start = instance.find("luid_")?;
    let tail = &instance[start..];
    let physical = tail.find("_phys_")? + "_phys_".len();
    let digits = tail[physical..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    if digits == 0 {
        return None;
    }
    let id = &tail[..physical + digits];
    let parts: Vec<_> = id.split('_').collect();
    if parts.len() != 5
        || parts[0] != "luid"
        || parts[3] != "phys"
        || !parts[1..3].iter().all(|part| {
            part.strip_prefix("0x").is_some_and(|hex| {
                !hex.is_empty() && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        })
    {
        return None;
    }
    Some(id)
}
fn engine_id(instance: &str) -> Option<(&str, &str)> {
    let adapter = adapter_id(instance)?;
    let tail = instance.split_once("_eng_")?.1;
    let number = tail.split('_').next()?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((adapter, number))
}
fn cook(first: &Snapshot, second: &Snapshot) -> Result<Vec<GpuSample>> {
    let old: HashMap<_, _> = first
        .engines
        .iter()
        .map(|engine| (engine.id.as_str(), engine))
        .collect();
    let mut adapters: BTreeMap<String, GpuSample> = BTreeMap::new();
    let mut engines: HashMap<(String, String), f64> = HashMap::new();
    for current in &second.engines {
        let Some((adapter, engine)) = engine_id(&current.id) else {
            continue;
        };
        adapters
            .entry(adapter.into())
            .or_insert_with(|| empty(adapter));
        let Some(previous) = old.get(current.id.as_str()) else {
            continue;
        };
        let ticks = current
            .ticks
            .parse::<u64>()?
            .checked_sub(previous.ticks.parse()?);
        let elapsed = current
            .clock
            .parse::<u64>()?
            .checked_sub(previous.clock.parse()?);
        if let Some((ticks, elapsed)) = ticks.zip(elapsed).filter(|(_, elapsed)| *elapsed > 0) {
            *engines.entry((adapter.into(), engine.into())).or_default() +=
                ticks as f64 / elapsed as f64 * 100.0;
        }
    }
    for ((adapter, _), percent) in engines {
        if let Some(gpu) = adapters.get_mut(&adapter) {
            gpu.busy_percent = Some(
                gpu.busy_percent
                    .map_or(percent, |old| old.max(percent))
                    .clamp(0.0, 100.0),
            );
        }
    }
    for memory in &second.memory {
        let Some(id) = adapter_id(&memory.id) else {
            continue;
        };
        adapters
            .entry(id.into())
            .or_insert_with(|| empty(id))
            .memory_used_bytes = Some(memory.used.parse().context("Invalid Windows GPU memory")?);
    }
    if adapters.is_empty() {
        // Inventory descriptors are Clone-free here so borrow-safe copying is explicit.
        return Ok(second
            .inventory
            .iter()
            .filter(|adapter| !adapter.id.is_empty())
            .map(|adapter| GpuSample {
                id: adapter.id.clone(),
                name: adapter.name.clone(),
                vendor: adapter.vendor.clone(),
                driver: (!adapter.driver.is_empty()).then(|| adapter.driver.clone()),
                busy_percent: None,
                memory_used_bytes: None,
                memory_total_bytes: None,
                temperature_celsius: None,
            })
            .collect());
    }
    Ok(adapters.into_values().collect())
}
fn empty(id: &str) -> GpuSample {
    GpuSample {
        id: id.into(),
        name: format!("GPU {id}"),
        vendor: "Unknown".into(),
        driver: None,
        busy_percent: None,
        memory_used_bytes: None,
        memory_total_bytes: None,
        temperature_celsius: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(ticks: [u64; 4], clock: u64) -> Snapshot {
        let names = [
            "pid_1_luid_0x0_0xa_phys_0_eng_0_engtype_3D",
            "pid_2_luid_0x0_0xa_phys_0_eng_0_engtype_3D",
            "pid_1_luid_0x0_0xa_phys_0_eng_1_engtype_Copy",
            "pid_1_luid_0x0_0xb_phys_0_eng_0_engtype_3D",
        ];
        Snapshot {
            supported: true,
            inventory_available: true,
            engines: names
                .into_iter()
                .zip(ticks)
                .map(|(id, ticks)| Engine {
                    id: id.into(),
                    ticks: ticks.to_string(),
                    clock: clock.to_string(),
                })
                .collect(),
            memory: vec![Memory {
                id: "luid_0x0_0xb_phys_0".into(),
                used: "9007199254740993".into(),
            }],
            inventory: vec![],
        }
    }
    #[test]
    fn aggregates_processes_per_engine_and_keeps_two_adapters_separate() -> Result<()> {
        let first = fixture([0, 0, 0, 0], 1000);
        let second = fixture([200, 300, 400, 800], 2000);
        let values = cook(&first, &second)?;
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].busy_percent, Some(50.0));
        assert_eq!(values[1].busy_percent, Some(80.0));
        assert_eq!(values[1].memory_used_bytes, Some(9_007_199_254_740_993));
        assert!(values.iter().all(|gpu| gpu.memory_total_bytes.is_none()));
        Ok(())
    }
    #[test]
    fn reset_new_engines_and_unsupported_inventory_are_not_zero_usage() -> Result<()> {
        let first = fixture([1000; 4], 2000);
        let second = fixture([0; 4], 1000);
        assert!(
            cook(&first, &second)?
                .iter()
                .all(|gpu| gpu.busy_percent.is_none())
        );
        assert!(adapter_id("luid_0x0_0xb_phys_文字").is_none());
        assert!(adapter_id("luid_not_hex_phys_0").is_none());
        let values = inventory(vec![Adapter {
            id: "PCI\\GPU".into(),
            name: "Virtual adapter".into(),
            vendor: "Virtio".into(),
            driver: "1".into(),
        }]);
        assert_eq!(values.len(), 1);
        assert!(values[0].busy_percent.is_none());
        Ok(())
    }
    #[test]
    fn missing_or_null_gpu_timers_are_not_zero_counters() {
        assert!(parse(r#"{"supported":true,"inventory_available":true,"engines":[{"id":"pid_1_luid_0x0_0xa_phys_0_eng_0","clock":"100"}],"memory":[],"inventory":[]}"#).is_err());
        assert!(parse(r#"{"supported":true,"inventory_available":true,"engines":[{"id":"pid_1_luid_0x0_0xa_phys_0_eng_0","ticks":"100","clock":null}],"memory":[],"inventory":[]}"#).is_err());
    }
}
