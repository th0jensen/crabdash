//! IOBlockStorageDriver's documented cumulative bytes are exact physical I/O counters.
use super::super::DiskCounter;
use super::super::collector::ResourceCollector;
use anyhow::{Context as _, Result};
use plist::Value;
use utils::{args, args::Args};

pub(crate) async fn sample(machine: &mut ResourceCollector<'_>) -> Result<Vec<DiskCounter>> {
    let output = machine
        .run(
            "ioreg",
            &args!["-a", "-r", "-c", "IOBlockStorageDriver", "-d", "1"],
        )
        .await?;
    parse(output.as_bytes())
}

fn parse(output: &[u8]) -> Result<Vec<DiskCounter>> {
    let nodes = Value::from_reader_xml(output).context("Invalid storage IORegistry response")?;
    let entries = nodes
        .as_array()
        .context("Invalid storage IORegistry roots")?;
    let mut counters = Vec::new();
    for node in entries {
        let Some(node) = node.as_dictionary() else {
            continue;
        };
        let Some(stats) = node.get("Statistics").and_then(Value::as_dictionary) else {
            continue;
        };
        let (Some(read_bytes), Some(written_bytes)) = (
            stats
                .get("Bytes (Read)")
                .and_then(Value::as_unsigned_integer),
            stats
                .get("Bytes (Write)")
                .and_then(Value::as_unsigned_integer),
        ) else {
            continue;
        };
        let id = node
            .get("IORegistryEntryID")
            .and_then(Value::as_unsigned_integer)
            .context("Missing storage registry identifier")?;
        let name = node
            .get("IORegistryEntryName")
            .and_then(Value::as_string)
            .unwrap_or("storage");
        counters.push(DiskCounter {
            id: format!("{name} · {id:x}"),
            read_bytes,
            written_bytes,
        });
    }
    Ok(counters)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_registry_ids_keep_two_storage_devices_distinct() -> Result<()> {
        let output = br#"<?xml version="1.0"?><plist version="1.0"><array>
        <dict><key>IORegistryEntryID</key><integer>100</integer><key>IORegistryEntryName</key><string>IOBlockStorageDriver</string><key>Statistics</key><dict><key>Bytes (Read)</key><integer>4096</integer><key>Bytes (Write)</key><integer>8192</integer></dict></dict>
        <dict><key>IORegistryEntryID</key><integer>101</integer><key>IORegistryEntryName</key><string>IOBlockStorageDriver</string><key>Statistics</key><dict><key>Bytes (Read)</key><integer>1024</integer><key>Bytes (Write)</key><integer>2048</integer></dict></dict>
        </array></plist>"#;
        let counters = parse(output)?;
        assert_eq!(counters.len(), 2);
        assert_ne!(counters[0].id, counters[1].id);
        assert_eq!(counters[0].read_bytes, 4096);
        assert_eq!(counters[1].written_bytes, 2048);
        assert!(parse(b"not XML").is_err());
        Ok(())
    }
}
