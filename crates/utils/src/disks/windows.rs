//! Parse the small stable JSON schema emitted by Windows disk discovery.
use super::{Disk, DiskNode, format_bytes};
use anyhow::{Context as _, Result};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsDisk {
    number: u32,
    #[serde(default)]
    name: String,
    size: u64,
    #[serde(default)]
    health: String,
    #[serde(default)]
    offline: bool,
    #[serde(default)]
    bus: String,
    #[serde(default)]
    style: String,
    #[serde(default)]
    partitions: Vec<Partition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Partition {
    number: u32,
    size: u64,
    #[serde(default, rename = "Type")]
    kind: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    file_system: String,
    #[serde(default)]
    label: String,
}

fn detail(values: &[&str]) -> Option<String> {
    let value = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    (!value.is_empty()).then_some(value)
}

pub fn parse(output: &str) -> Result<Vec<Disk>> {
    let disks: Vec<WindowsDisk> = serde_json::from_str(output.trim_start_matches('\u{feff}'))
        .context("Invalid Windows disk response")?;
    Ok(disks
        .into_iter()
        .map(|disk| Disk {
            id: format!("Disk {}", disk.number),
            name: if disk.name.is_empty() {
                format!("Disk {}", disk.number)
            } else {
                disk.name
            },
            size: Some(format_bytes(disk.size)),
            status: if disk.offline {
                "offline".into()
            } else {
                disk.health.to_ascii_lowercase()
            },
            detail: detail(&[&disk.bus, &disk.style]),
            nodes: disk
                .partitions
                .into_iter()
                .map(|partition| {
                    let paths = partition
                        .paths
                        .into_iter()
                        .filter(|path| !path.starts_with("\\\\?\\Volume{"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    DiskNode {
                        name: format!("Partition {}", partition.number),
                        size: Some(format_bytes(partition.size)),
                        detail: detail(&[
                            &partition.kind,
                            &partition.file_system,
                            &partition.label,
                            &paths,
                        ]),
                        nodes: Vec::new(),
                    }
                })
                .collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_partitioned_and_offline_disks() -> Result<()> {
        let disks = parse(
            r#"[{"Number":0,"Name":"NVMe","Size":1000000000,"Health":"Healthy","Bus":"NVMe","Style":"GPT","Partitions":[{"Number":1,"Size":1000000,"Type":"Basic","Paths":["C:\\"],"FileSystem":"NTFS","Label":"Windows"}]},{"Number":2,"Name":"USB","Size":64000000,"Health":"Healthy","Offline":true}]"#,
        )?;
        assert_eq!(disks.len(), 2);
        assert!(disks[0].is_healthy());
        assert_eq!(
            disks[0].nodes[0].detail.as_deref(),
            Some("Basic · NTFS · Windows · C:\\")
        );
        assert_eq!(disks[1].status, "offline");
        assert!(parse("[]")?.is_empty());
        assert!(parse("Access denied").is_err());
        Ok(())
    }
}
