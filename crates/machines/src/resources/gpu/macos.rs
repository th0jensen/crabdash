//! IORegistry identities and optional, explicitly labelled driver statistics.
//! These statistics are not a portable macOS API: missing values stay unknown.
use super::super::GpuSample;
use super::super::collector::ResourceCollector;
use anyhow::{Context as _, Result, ensure};
use plist::Value;
use utils::{args, args::Args};

pub(crate) async fn sample(machine: &mut ResourceCollector<'_>) -> Result<Vec<GpuSample>> {
    let first = machine
        .run(
            "/usr/sbin/ioreg",
            &args!["-a", "-r", "-c", "IOAccelerator", "-d", "1"],
        )
        .await?;
    let entries = parse(&first)?;
    if !entries.is_empty() {
        return Ok(entries);
    }
    let fallback = machine
        .run(
            "/usr/sbin/ioreg",
            &args!["-a", "-r", "-c", "IOGPU", "-d", "1"],
        )
        .await?;
    let entries = parse(&fallback)?;
    ensure!(!entries.is_empty(), "GPU telemetry provider unavailable");
    Ok(entries)
}
fn parse(output: &str) -> Result<Vec<GpuSample>> {
    let value =
        Value::from_reader_xml(output.as_bytes()).context("Invalid macOS GPU registry response")?;
    let rows = value
        .as_array()
        .context("Invalid macOS GPU registry array")?;
    let mut entries = Vec::new();
    for value in rows {
        let row = value
            .as_dictionary()
            .context("Invalid GPU registry entry")?;
        let id = row
            .get("IORegistryEntryID")
            .and_then(Value::as_unsigned_integer)
            .context("Missing GPU registry identity")?;
        let name = row
            .get("IORegistryEntryName")
            .and_then(Value::as_string)
            .or_else(|| row.get("IOObjectClass").and_then(Value::as_string))
            .unwrap_or("GPU");
        let class = row
            .get("IOObjectClass")
            .and_then(Value::as_string)
            .unwrap_or(name);
        let lower = class.to_ascii_lowercase();
        let vendor = if lower.contains("agx") || lower.contains("apple") {
            "Apple"
        } else if lower.contains("intel") {
            "Intel"
        } else if lower.contains("amd") || lower.contains("ati") {
            "AMD"
        } else if lower.contains("nvidia") || lower.contains("geforce") {
            "NVIDIA"
        } else {
            "Unknown"
        };
        // Renderer and tiler can overlap. Their sum is not device utilization.
        let busy_percent = row
            .get("PerformanceStatistics")
            .and_then(Value::as_dictionary)
            .and_then(|stats| stats.get("Device Utilization %"))
            .and_then(|value| {
                value
                    .as_real()
                    .or_else(|| value.as_unsigned_integer().map(|value| value as f64))
            })
            .filter(|value| value.is_finite() && (0.0..=100.0).contains(value));
        entries.push(GpuSample {
            id: format!("ioreg:{id:x}"),
            name: name.into(),
            vendor: vendor.into(),
            driver: Some(class.into()),
            busy_percent,
            memory_used_bytes: None,
            memory_total_bytes: None,
            temperature_celsius: None,
        });
    }
    Ok(entries)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dual_adapters_keep_ids_and_unsupported_statistics_remain_unknown() -> Result<()> {
        let xml = r#"<?xml version="1.0"?><plist version="1.0"><array>
        <dict><key>IORegistryEntryID</key><integer>100</integer><key>IORegistryEntryName</key><string>IntelAccelerator</string><key>IOObjectClass</key><string>IntelAccelerator</string><key>PerformanceStatistics</key><dict><key>Device Utilization %</key><integer>25</integer><key>Renderer Utilization %</key><integer>75</integer></dict></dict>
        <dict><key>IORegistryEntryID</key><integer>200</integer><key>IORegistryEntryName</key><string>AMDRadeon</string><key>IOObjectClass</key><string>AMDRadeon</string><key>PerformanceStatistics</key><dict><key>Renderer Utilization %</key><integer>30</integer></dict></dict>
        </array></plist>"#;
        let entries = parse(xml)?;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "ioreg:64");
        assert_eq!(entries[1].id, "ioreg:c8");
        assert_eq!(entries[0].busy_percent, Some(25.0));
        assert_eq!(entries[1].busy_percent, None);
        assert!(
            entries
                .iter()
                .all(|entry| entry.memory_total_bytes.is_none())
        );
        assert!(parse("not plist").is_err());
        Ok(())
    }
}
