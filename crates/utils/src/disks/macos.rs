use super::format_bytes;
use super::{Disk, DiskNode, collect_mount_points, device_path, mounted};
use crate::output::Output;
use anyhow::{Context as _, Result};
use plist::from_bytes;
use serde::Deserialize;
#[derive(Debug, Deserialize)]
pub struct DiskutilList {
    #[serde(rename = "AllDisksAndPartitions")]
    pub all_disks_and_partitions: Vec<DiskutilEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiskutilEntry {
    #[serde(rename = "APFSPhysicalStores", default)]
    pub apfs_physical_stores: Vec<DiskutilPhysicalStore>,
    #[serde(rename = "APFSVolumes", default)]
    pub apfs_volumes: Vec<DiskutilVolume>,
    #[serde(rename = "DeviceIdentifier")]
    pub device_identifier: String,
    #[serde(rename = "OSInternal")]
    pub os_internal: Option<bool>,
    #[serde(rename = "Partitions", default)]
    pub partitions: Vec<DiskutilPartition>,
    #[serde(rename = "Size")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiskutilPhysicalStore {
    #[serde(rename = "DeviceIdentifier")]
    pub device_identifier: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiskutilPartition {
    #[serde(rename = "Content")]
    content: Option<String>,
    #[serde(rename = "DeviceIdentifier")]
    device_identifier: String,
    #[serde(rename = "Size")]
    size: Option<u64>,
    #[serde(rename = "VolumeName")]
    volume_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiskutilVolume {
    #[serde(rename = "CapacityInUse")]
    capacity_in_use: Option<u64>,
    #[serde(rename = "DeviceIdentifier")]
    device_identifier: String,
    #[serde(rename = "MountPoint")]
    mount_point: Option<String>,
    #[serde(rename = "Size")]
    size: Option<u64>,
    #[serde(rename = "VolumeName")]
    volume_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiskutilApfsList {
    #[serde(rename = "Containers", default)]
    containers: Vec<DiskutilApfsContainer>,
}

#[derive(Debug, Deserialize)]
struct DiskutilApfsContainer {
    #[serde(rename = "Volumes", default)]
    volumes: Vec<DiskutilApfsVolume>,
}

#[derive(Debug, Deserialize)]
struct DiskutilApfsVolume {
    #[serde(rename = "DeviceIdentifier")]
    device_identifier: String,
    #[serde(rename = "Roles", default)]
    roles: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DiskutilInfo {
    #[serde(rename = "BusProtocol")]
    bus_protocol: Option<String>,
    #[serde(rename = "IORegistryEntryName")]
    io_registry_entry_name: Option<String>,
    #[serde(rename = "Internal")]
    internal: Option<bool>,
    #[serde(rename = "MediaName")]
    media_name: Option<String>,
    #[serde(rename = "RemovableMediaOrExternalDevice")]
    removable_media_or_external_device: Option<bool>,
}

impl Disk {
    pub fn apply_diskutil_info(&mut self, stdout: &str) -> Result<()> {
        let info: DiskutilInfo =
            from_bytes(stdout.as_bytes()).context("failed to parse diskutil info plist output")?;

        let mut parts = Vec::new();

        if let Some(name) = info
            .media_name
            .or(info.io_registry_entry_name)
            .filter(|value| !value.trim().is_empty() && value != &self.name)
        {
            parts.push(name);
        }

        if let Some(location) = match info.internal {
            Some(true) => Some("internal"),
            _ if info.removable_media_or_external_device == Some(true) => Some("external"),
            _ => None,
        } {
            parts.push(location.to_string());
        }

        self.detail = (!parts.is_empty()).then(|| parts.join(" • "));

        if info
            .bus_protocol
            .as_deref()
            .is_some_and(|value| value == "Disk Image")
        {
            self.flatten_disk_image_nodes();
        }

        Ok(())
    }
    fn flatten_disk_image_nodes(&mut self) {
        while self.nodes.len() == 1
            && (self.nodes[0].name == "Container" || !self.nodes[0].nodes.is_empty())
        {
            let node = self.nodes.remove(0);
            if node.nodes.is_empty() {
                self.nodes.push(node);
                break;
            }

            self.nodes = node.nodes;
        }

        if self.nodes.len() == 1 && self.nodes[0].nodes.is_empty() {
            let node = self.nodes.remove(0);
            self.name = node.name;
            if self.size.is_none() {
                self.size = node.size;
            }
            if let Some(detail) = node.detail.filter(|value| !value.trim().is_empty()) {
                match &mut self.detail {
                    Some(existing) if !existing.contains(&detail) => {
                        existing.push_str(" • ");
                        existing.push_str(&detail);
                    }
                    None => self.detail = Some(detail),
                    _ => {}
                }
            }
        }
    }
}

pub fn diskutil_nodes(
    partitions: &[DiskutilPartition],
    containers: &std::collections::HashMap<String, DiskutilEntry>,
    apfs_roles: &std::collections::HashMap<String, String>,
) -> (Vec<DiskNode>, bool) {
    let mut active = false;
    let nodes = partitions
        .iter()
        .filter_map(|partition| {
            if let Some(container) = containers.get(&partition.device_identifier) {
                let (container_nodes, container_active) =
                    diskutil_volume_nodes(&container.apfs_volumes, apfs_roles);

                if container_nodes.is_empty() {
                    return None;
                }

                active |= container_active;

                Some(DiskNode {
                    name: String::from("Container"),
                    size: partition.size.map(format_bytes),
                    detail: Some(String::from("APFS container")),
                    nodes: container_nodes,
                })
            } else if partition_is_visible(partition) {
                Some(DiskNode {
                    name: partition_label(partition),
                    size: partition.size.map(format_bytes),
                    detail: partition_detail(partition),
                    nodes: Vec::new(),
                })
            } else {
                None
            }
        })
        .collect();

    (nodes, active)
}

pub fn diskutil_volume_nodes(
    volumes: &[DiskutilVolume],
    apfs_roles: &std::collections::HashMap<String, String>,
) -> (Vec<DiskNode>, bool) {
    volumes
        .iter()
        .filter_map(|volume| {
            let role = apfs_roles
                .get(&volume.device_identifier)
                .cloned()
                .unwrap_or_else(|| String::from("Volume"));
            let name = volume
                .volume_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty() && !is_hidden_macos_role(&role))?;
            let mounted = mounted(&collect_mount_points(volume.mount_point.as_deref()));

            Some((
                DiskNode {
                    name: name.to_string(),
                    size: volume.capacity_in_use.or(volume.size).map(format_bytes),
                    detail: Some(macos_volume_detail(&role, volume.mount_point.as_deref())),
                    nodes: Vec::new(),
                },
                mounted,
            ))
        })
        .fold(
            (Vec::new(), false),
            |(mut nodes, active), (node, mounted)| {
                nodes.push(node);
                (nodes, active || mounted)
            },
        )
}

pub fn preferred_diskutil_name(nodes: &[DiskNode]) -> Option<String> {
    nodes.iter().find_map(|node| {
        preferred_diskutil_name(&node.nodes).or_else(|| {
            (!node.name.eq_ignore_ascii_case("container")
                && !node
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail == "APFS container"))
            .then(|| node.name.clone())
        })
    })
}

pub fn parse_diskutil_apfs_roles(
    stdout: Output,
) -> Result<std::collections::HashMap<String, String>> {
    let parsed: DiskutilApfsList =
        from_bytes(&stdout.0).context("failed to parse diskutil apfs plist output")?;

    Ok(parsed
        .containers
        .into_iter()
        .flat_map(|container| container.volumes.into_iter())
        .filter_map(|volume| {
            volume
                .roles
                .first()
                .cloned()
                .map(|role| (volume.device_identifier, role))
        })
        .collect())
}

fn partition_label(partition: &DiskutilPartition) -> String {
    if let Some(name) = partition
        .volume_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return name.to_string();
    }

    match partition.content.as_deref().unwrap_or_default() {
        "Apple_APFS_ISC" => String::from("iSC"),
        "Apple_APFS_Recovery" => String::from("Recovery"),
        "Apple_APFS" => String::from("Container"),
        content if !content.is_empty() => content
            .trim_start_matches("Apple_")
            .replace('_', " ")
            .replace("APFS", "APFS "),
        _ => partition.device_identifier.clone(),
    }
}

fn partition_is_visible(partition: &DiskutilPartition) -> bool {
    partition
        .volume_name
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty() && !is_hidden_macos_name(value))
}

fn is_hidden_macos_name(name: &str) -> bool {
    matches!(
        name,
        "iSCPreboot" | "xART" | "Hardware" | "Preboot" | "Recovery" | "Update" | "VM"
    )
}

fn is_hidden_macos_role(role: &str) -> bool {
    matches!(
        role,
        "Preboot" | "Recovery" | "Update" | "VM" | "xART" | "Hardware"
    )
}

fn partition_detail(partition: &DiskutilPartition) -> Option<String> {
    partition.content.as_deref().map(|content| match content {
        "EFI" => String::from("EFI partition"),
        "Apple_APFS_ISC" => String::from("APFS iSC partition"),
        "Apple_APFS_Recovery" => String::from("APFS recovery partition"),
        value => format!(
            "{} partition",
            value.trim_start_matches("Apple_").replace('_', " ")
        ),
    })
}

fn macos_volume_detail(role: &str, mount_point: Option<&str>) -> String {
    let mount_point = mount_point.map(str::trim).filter(|value| !value.is_empty());

    let base = if role == "Volume" {
        None
    } else {
        Some(format!("{} volume", role.to_ascii_lowercase()))
    };

    match (base, mount_point) {
        (Some(base), Some(path)) => format!("{base} • {path}"),
        (Some(base), None) => base,
        (None, Some(path)) => format!("{path}"),
        (None, None) => String::from("volume"),
    }
}

pub fn parse(output: Output, apfs_stdout: Option<Output>) -> Result<Vec<Disk>> {
    let parsed: DiskutilList = from_bytes(&output.0)?;
    let apfs_roles = apfs_stdout
        .map(parse_diskutil_apfs_roles)
        .transpose()?
        .unwrap_or_default();

    let containers = parsed
        .all_disks_and_partitions
        .iter()
        .filter(|entry| !entry.apfs_physical_stores.is_empty())
        .flat_map(|entry| {
            entry
                .apfs_physical_stores
                .iter()
                .map(|store| (store.device_identifier.clone(), entry.clone()))
        })
        .collect::<std::collections::HashMap<_, _>>();

    Ok(parsed
        .all_disks_and_partitions
        .into_iter()
        .filter(|entry| entry.apfs_physical_stores.is_empty())
        .map(|entry| {
            let (nodes, active) = diskutil_nodes(&entry.partitions, &containers, &apfs_roles);
            let name =
                preferred_diskutil_name(&nodes).unwrap_or_else(|| entry.device_identifier.clone());
            Disk {
                id: device_path(&entry.device_identifier),
                name,
                size: entry.size.map(format_bytes),
                status: if active {
                    String::from("mounted")
                } else {
                    String::from("healthy")
                },
                detail: entry
                    .os_internal
                    .and_then(|is_internal: bool| is_internal.then_some(String::from("internal"))),
                nodes,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_apfs_roles_to_physical_disks_and_keeps_mounted_volumes() {
        let list = Output::from(r#"<?xml version="1.0"?><plist version="1.0"><dict>
            <key>AllDisksAndPartitions</key><array>
                <dict><key>DeviceIdentifier</key><string>disk0</string><key>OSInternal</key><true/>
                    <key>Partitions</key><array><dict><key>DeviceIdentifier</key><string>disk0s1</string></dict></array></dict>
                <dict><key>DeviceIdentifier</key><string>disk3</string>
                    <key>APFSPhysicalStores</key><array><dict><key>DeviceIdentifier</key><string>disk0s1</string></dict></array>
                    <key>APFSVolumes</key><array><dict><key>DeviceIdentifier</key><string>disk3s1</string>
                        <key>VolumeName</key><string>System</string><key>MountPoint</key><string>/</string></dict></array></dict>
            </array></dict></plist>"#.to_string());
        let roles = Output::from(
            r#"<?xml version="1.0"?><plist version="1.0"><dict><key>Containers</key><array>
            <dict><key>Volumes</key><array><dict><key>DeviceIdentifier</key><string>disk3s1</string>
                <key>Roles</key><array><string>System</string></array></dict></array></dict>
            </array></dict></plist>"#
                .to_string(),
        );
        let disks = parse(list, Some(roles)).expect("valid diskutil plist");
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].id, "/dev/disk0");
        assert_eq!(disks[0].name, "System");
        assert_eq!(disks[0].status, "mounted");
        assert_eq!(disks[0].detail.as_deref(), Some("internal"));
        assert_eq!(
            disks[0].nodes[0].nodes[0].detail.as_deref(),
            Some("system volume • /")
        );
    }
}
