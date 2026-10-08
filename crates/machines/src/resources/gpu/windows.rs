//! WDDM engine timers grouped by exact LUID/physical-adapter identity.
//! Different engines can execute concurrently: overall activity is the busiest
//! engine, not the sum of the 3D/copy/video percentages.
use super::super::GpuSample;
use super::super::collector::ResourceCollector;
use anyhow::{Context as _, Result, ensure};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

const INVENTORY: &str = include_str!("windows/inventory.cs");
const PROBE: &str = include_str!("windows/probe.ps1");

fn script(discover: bool) -> String {
    // Static source is a literal here-string, never interpolated target/user data.
    if discover {
        format!(
            "$crabdash_gpu_discover = $true\n$crabdash_gpu_inventory = @'\n{INVENTORY}\n'@\n{PROBE}"
        )
    } else {
        format!("$crabdash_gpu_discover = $false\n{PROBE}")
    }
}

pub(crate) async fn sample(machine: &mut ResourceCollector<'_>) -> Result<Vec<GpuSample>> {
    let first = parse(&machine.powershell(&script(false)).await?)?;
    if first.supported {
        machine.delay(Duration::from_secs(1)).await?;
    }
    // Inventory is discovered once per sample, after the counter baseline. This
    // retains hotplug handling without compiling the native helper twice.
    let second = parse(&machine.powershell(&script(true)).await?)?;
    if !first.supported || !second.supported {
        return cook(&second, &second);
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
struct NativeAdapter {
    id: String,
    pnp_id: Option<String>,
    name: Option<String>,
    vendor: Option<String>,
    capacity: Option<String>,
}
#[derive(Deserialize)]
struct Snapshot {
    #[serde(default)]
    native_available: bool,
    #[serde(default)]
    adapters: Vec<NativeAdapter>,
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
// Both discovery and performance IDs normalize their numeric LUID/physical fields.
// Names/order/PCI model IDs are never identity joins (identical cards are valid).
fn discovered(snapshot: &Snapshot) -> Result<BTreeMap<String, GpuSample>> {
    let mut values = BTreeMap::new();
    let mut claimed = std::collections::HashSet::new();
    for adapter in &snapshot.adapters {
        let Some(id) = adapter_id(&adapter.id) else {
            continue;
        };
        let descriptor = adapter.pnp_id.as_ref().and_then(|pnp| {
            snapshot
                .inventory
                .iter()
                .find(|item| item.id.eq_ignore_ascii_case(pnp))
        });
        if let Some(item) = descriptor {
            claimed.insert(item.id.to_ascii_uppercase());
        }
        let mut gpu = empty(&id);
        gpu.name = descriptor
            .map(|item| item.name.as_str())
            .filter(|name| !name.is_empty())
            .or(adapter.name.as_deref().filter(|name| !name.is_empty()))
            .map_or(gpu.name, str::to_owned);
        gpu.vendor = descriptor
            .map(|item| item.vendor.as_str())
            .filter(|vendor| !vendor.is_empty())
            .or(adapter
                .vendor
                .as_deref()
                .filter(|vendor| !vendor.is_empty()))
            .map_or(gpu.vendor, str::to_owned);
        gpu.driver =
            descriptor.and_then(|item| (!item.driver.is_empty()).then(|| item.driver.clone()));
        gpu.memory_total_bytes = adapter
            .capacity
            .as_deref()
            .map(str::parse::<u64>)
            .transpose()
            .context("Invalid Windows GPU capacity")?;
        values.insert(id, gpu);
    }
    // Unmapped PNP devices remain visible, without attaching somebody else's counters.
    for adapter in &snapshot.inventory {
        if adapter.id.is_empty() || claimed.contains(&adapter.id.to_ascii_uppercase()) {
            continue;
        }
        let mut gpu = empty(&adapter.id);
        gpu.name = adapter.name.clone();
        gpu.vendor = adapter.vendor.clone();
        gpu.driver = (!adapter.driver.is_empty()).then(|| adapter.driver.clone());
        values.insert(adapter.id.clone(), gpu);
    }
    Ok(values)
}
fn adapter_id(instance: &str) -> Option<String> {
    let start = instance.find("luid_")?;
    let mut parts = instance[start..].split('_');
    if parts.next()? != "luid" {
        return None;
    }
    let high = u32::from_str_radix(parts.next()?.strip_prefix("0x")?, 16).ok()?;
    let low = u32::from_str_radix(parts.next()?.strip_prefix("0x")?, 16).ok()?;
    if parts.next()? != "phys" {
        return None;
    }
    let physical = parts.next()?.parse::<u32>().ok()?;
    Some(format!("luid_0x{high:x}_0x{low:x}_phys_{physical}"))
}
fn engine_id(instance: &str) -> Option<(String, u32)> {
    let adapter = adapter_id(instance)?;
    let tail = instance.split_once("_eng_")?.1;
    let number = tail.split('_').next()?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((adapter, number.parse().ok()?))
}
fn counter_key(instance: &str) -> Option<(String, String)> {
    let adapter = adapter_id(instance)?;
    let (process, _) = instance.split_once("luid_")?;
    let (_, tail) = instance.split_once("_eng_")?;
    let (number, suffix) = tail.split_once('_').map_or((tail, ""), |parts| parts);
    let number = number.parse::<u32>().ok()?;
    Some((adapter, format!("{process}eng_{number}_{suffix}")))
}
fn cook(first: &Snapshot, second: &Snapshot) -> Result<Vec<GpuSample>> {
    let old: HashMap<_, _> = first
        .engines
        .iter()
        .filter_map(|engine| counter_key(&engine.id).map(|key| (key, engine)))
        .collect();
    let mut adapters = discovered(second)?;
    let mut engines: HashMap<(String, u32), f64> = HashMap::new();
    for current in &second.engines {
        let Some((adapter, engine)) = engine_id(&current.id) else {
            continue;
        };
        adapters
            .entry(adapter.clone())
            .or_insert_with(|| empty(&adapter));
        let Some(key) = counter_key(&current.id) else {
            continue;
        };
        let Some(previous) = old.get(&key) else {
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
            *engines.entry((adapter, engine)).or_default() += ticks as f64 / elapsed as f64 * 100.0;
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
            .entry(id.clone())
            .or_insert_with(|| empty(&id))
            .memory_used_bytes = Some(memory.used.parse().context("Invalid Windows GPU memory")?);
    }
    ensure!(
        !adapters.is_empty() || second.inventory_available || second.native_available,
        "GPU inventory and telemetry providers unavailable"
    );
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
            native_available: false,
            adapters: vec![],
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
        let values = discovered(&Snapshot {
            supported: false,
            inventory_available: true,
            native_available: false,
            adapters: vec![],
            engines: vec![],
            memory: vec![],
            inventory: vec![Adapter {
                id: "PCI\\GPU".into(),
                name: "Virtual adapter".into(),
                vendor: "Virtio".into(),
                driver: "1".into(),
            }],
        })?;
        assert_eq!(values.len(), 1);
        assert!(values.values().all(|gpu| gpu.busy_percent.is_none()));
        Ok(())
    }
    #[test]
    fn missing_or_null_gpu_timers_are_not_zero_counters() {
        assert!(parse(r#"{"supported":true,"inventory_available":true,"engines":[{"id":"pid_1_luid_0x0_0xa_phys_0_eng_0","clock":"100"}],"memory":[],"inventory":[]}"#).is_err());
        assert!(parse(r#"{"supported":true,"inventory_available":true,"engines":[{"id":"pid_1_luid_0x0_0xa_phys_0_eng_0","ticks":"100","clock":null}],"memory":[],"inventory":[]}"#).is_err());
    }
    fn no_counters() -> Snapshot {
        Snapshot {
            supported: false,
            inventory_available: false,
            native_available: false,
            adapters: vec![],
            engines: vec![],
            memory: vec![],
            inventory: vec![],
        }
    }
    fn native(id: &str, pnp: Option<&str>, capacity: Option<&str>) -> NativeAdapter {
        NativeAdapter {
            id: id.into(),
            pnp_id: pnp.map(str::to_owned),
            name: None,
            vendor: None,
            capacity: capacity.map(str::to_owned),
        }
    }
    fn descriptor(id: &str, name: &str) -> Adapter {
        Adapter {
            id: id.into(),
            name: name.into(),
            vendor: "NVIDIA".into(),
            driver: "555".into(),
        }
    }
    #[test]
    fn exact_identity_keeps_idle_same_name_adapter_and_reordered_inventory() -> Result<()> {
        let mut first = fixture([0; 4], 1000);
        first.engines.truncate(3);
        let mut second = fixture([200, 300, 400, 0], 2000);
        second.engines.truncate(3);
        second.memory.clear();
        second.inventory = vec![
            descriptor("PCI\\B", "Same GPU"),
            descriptor("PCI\\A", "Same GPU"),
        ];
        second.adapters = vec![
            native(
                "luid_0x00000000_0x0000000A_phys_00",
                Some("pci\\a"),
                Some("8589934592"),
            ),
            native("luid_0x0_0xb_phys_0", Some("PCI\\B"), Some("17179869184")),
        ];
        let values = cook(&first, &second)?;
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].name, "Same GPU");
        assert_eq!(values[1].name, "Same GPU");
        assert_ne!(values[0].id, values[1].id);
        assert_eq!(values[0].busy_percent, Some(50.0));
        assert!(values[1].busy_percent.is_none());
        assert_eq!(values[0].memory_total_bytes, Some(8_589_934_592));
        assert_eq!(values[1].memory_total_bytes, Some(17_179_869_184));
        assert!(
            values
                .iter()
                .all(|gpu| gpu.driver.as_deref() == Some("555"))
        );
        Ok(())
    }
    #[test]
    fn linked_physical_cards_have_independent_capacity_and_counters() -> Result<()> {
        let mut first = fixture([0; 4], 1000);
        first.engines.truncate(1);
        let mut second = no_counters();
        second.adapters = vec![
            native(
                "luid_0x0_0xa_phys_0",
                Some("PCI\\A"),
                Some("9007199254740993"),
            ),
            native("luid_0x0_0xa_phys_1", Some("PCI\\B"), None),
        ];
        second.inventory = vec![
            descriptor("PCI\\B", "Card B"),
            descriptor("PCI\\A", "Card A"),
        ];
        second.engines = vec![
            Engine {
                id: first.engines[0].id.clone(),
                ticks: "250".into(),
                clock: "2000".into(),
            },
            Engine {
                id: "pid_1_luid_0x0_0xa_phys_1_eng_0_engtype_3D".into(),
                ticks: "900".into(),
                clock: "2000".into(),
            },
        ];
        second.memory = vec![Memory {
            id: "luid_0x0_0xa_phys_1".into(),
            used: "777".into(),
        }];
        let values = cook(&first, &second)?;
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].name, "Card A");
        assert_eq!(values[0].busy_percent, Some(25.0));
        assert_eq!(values[0].memory_total_bytes, Some(9_007_199_254_740_993));
        assert_eq!(values[1].name, "Card B");
        assert!(values[1].busy_percent.is_none());
        assert!(values[1].memory_total_bytes.is_none());
        assert_eq!(values[1].memory_used_bytes, Some(777));
        Ok(())
    }
    #[test]
    fn unmapped_inventory_and_counter_devices_are_not_guessed_by_order() -> Result<()> {
        let first = fixture([0; 4], 1000);
        let mut second = fixture([100; 4], 2000);
        second.inventory = vec![
            descriptor("PCI\\A", "Card A"),
            descriptor("PCI\\B", "Card B"),
        ];
        let values = cook(&first, &second)?;
        assert_eq!(values.len(), 4);
        let named = values
            .iter()
            .filter(|gpu| gpu.id.starts_with("PCI"))
            .collect::<Vec<_>>();
        assert_eq!(named.len(), 2);
        assert!(
            named
                .iter()
                .all(|gpu| gpu.busy_percent.is_none() && gpu.memory_used_bytes.is_none())
        );
        assert!(
            values
                .iter()
                .filter(|gpu| gpu.id.starts_with("luid"))
                .all(|gpu| gpu.name.starts_with("GPU luid") && gpu.driver.is_none())
        );
        Ok(())
    }
    #[test]
    fn empty_success_differs_from_failed_inventory_even_with_supported_timers() -> Result<()> {
        let mut snapshot = no_counters();
        snapshot.supported = true;
        assert!(cook(&snapshot, &snapshot).is_err());
        snapshot.inventory_available = true;
        assert!(cook(&snapshot, &snapshot)?.is_empty());
        snapshot.inventory_available = false;
        snapshot.native_available = true;
        assert!(cook(&snapshot, &snapshot)?.is_empty());
        Ok(())
    }
    #[test]
    fn normalized_counter_identity_survives_hex_padding_without_aliasing_phys() -> Result<()> {
        let mut first = fixture([0; 4], 1000);
        first.engines.truncate(1);
        first.engines[0].id = "pid_1_luid_0x00000000_0x0000000A_phys_00_eng_00_engtype_3D".into();
        let mut second = fixture([200; 4], 2000);
        second.engines.truncate(1);
        second.memory.clear();
        assert_eq!(cook(&first, &second)?[0].busy_percent, Some(20.0));
        assert_ne!(
            adapter_id("luid_0x0_0xa_phys_0"),
            adapter_id("luid_0x0_0xa_phys_1")
        );
        assert!(adapter_id("luid_0x100000000_0xa_phys_0").is_none());
        Ok(())
    }
    #[test]
    fn json_metadata_preserves_unicode_and_integer_precision() -> Result<()> {
        let snapshot = parse(
            r#"{"supported":false,"inventory_available":true,"native_available":true,"engines":[],"memory":[],"inventory":[{"id":"PCI\\A","name":"GPU \"名前\"\nLine","vendor":"Vendor","driver":"1"}],"adapters":[{"id":"luid_0x0_0xa_phys_0","pnp_id":"PCI\\A","name":null,"vendor":null,"capacity":"18446744073709551615"}]}"#,
        )?;
        let values = cook(&snapshot, &snapshot)?;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].name, "GPU \"名前\"\nLine");
        assert_eq!(values[0].memory_total_bytes, Some(u64::MAX));
        Ok(())
    }
}
