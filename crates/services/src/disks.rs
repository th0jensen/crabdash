use anyhow::{Context, Result};
use plist::from_bytes;
use serde::Deserialize;

#[derive(Clone, Debug, Default)]
pub struct Disk {
    pub id: String,
    pub name: String,
    pub size: Option<String>,
    pub status: String,
    pub detail: Option<String>,
    pub nodes: Vec<DiskNode>,
}

#[derive(Clone, Debug, Default)]
pub struct DiskNode {
    pub id: Option<String>,
    pub name: String,
    pub size: Option<String>,
    pub status: Option<String>,
    pub detail: Option<String>,
    pub nodes: Vec<DiskNode>,
}

impl DiskNode {
    pub fn is_mountable(&self) -> bool {
        self.id.is_some()
            && !matches!(
                self.status.as_deref(),
                Some("swap") | Some("container") | None
            )
    }

    pub fn is_mounted(&self) -> bool {
        self.status.as_deref() == Some("mounted")
    }
}

impl Disk {
    pub fn is_healthy(&self) -> bool {
        matches!(self.status.as_str(), "healthy" | "mounted" | "swap")
    }

    pub fn convert_diskutil(stdout: &str, apfs_stdout: Option<&str>) -> Result<Vec<Disk>> {
        let parsed: DiskutilList =
            from_bytes(stdout.as_bytes()).context("failed to parse diskutil plist output")?;
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
                let name = preferred_diskutil_name(&nodes)
                    .unwrap_or_else(|| entry.device_identifier.clone());
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
                        .and_then(|is_internal| is_internal.then_some(String::from("internal"))),
                    nodes,
                }
            })
            .collect())
    }

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

    pub fn convert_lsblk(stdout: &str) -> Vec<Disk> {
        let rows = stdout
            .lines()
            .filter_map(parse_lsblk_row)
            .collect::<Vec<_>>();
        let children = rows.iter().enumerate().fold(
            std::collections::HashMap::<String, Vec<usize>>::new(),
            |mut map, (index, row)| {
                if let Some(parent) = row.pkname.clone() {
                    map.entry(parent).or_default().push(index);
                }
                map
            },
        );

        let mut disks = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.device_type == "disk")
            .map(|(_, row)| {
                let (nodes, active) = lsblk_children(&row.name, &rows, &children);
                let (name, extra_detail) = linux_disk_name(row);
                Disk {
                    id: row.path.clone(),
                    name,
                    size: row.size.clone(),
                    status: if active || mounted(&row.mount_points) {
                        String::from("mounted")
                    } else {
                        String::from("healthy")
                    },
                    detail: linux_disk_detail(row, extra_detail, &nodes),
                    nodes,
                }
            })
            .collect::<Vec<_>>();

        disks.sort_by_key(|disk| linux_disk_sort_key(disk));
        disks
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiskAction {
    Mount,
    Unmount,
}

impl DiskAction {
    pub fn pending_label(self) -> &'static str {
        match self {
            Self::Mount => "Mounting",
            Self::Unmount => "Unmounting",
        }
    }
}

pub trait Disks {
    /// Lists all disks connected to the machine
    ///
    /// # Returns
    /// * `Ok(Vec<Disk>)`: The disks connected to the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_disks(&mut self) -> Result<Vec<Disk>>;

    /// Mounts a disk by device ID.
    fn mount_disk(&mut self, id: &str) -> Result<()>;

    /// Unmounts a disk by device ID.
    fn unmount_disk(&mut self, id: &str) -> Result<()>;
}

#[derive(Debug, Deserialize)]
struct DiskutilList {
    #[serde(rename = "AllDisksAndPartitions")]
    all_disks_and_partitions: Vec<DiskutilEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct DiskutilEntry {
    #[serde(rename = "APFSPhysicalStores", default)]
    apfs_physical_stores: Vec<DiskutilPhysicalStore>,
    #[serde(rename = "APFSVolumes", default)]
    apfs_volumes: Vec<DiskutilVolume>,
    #[serde(rename = "DeviceIdentifier")]
    device_identifier: String,
    #[serde(rename = "OSInternal")]
    os_internal: Option<bool>,
    #[serde(rename = "Partitions", default)]
    partitions: Vec<DiskutilPartition>,
    #[serde(rename = "Size")]
    size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct DiskutilPhysicalStore {
    #[serde(rename = "DeviceIdentifier")]
    device_identifier: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DiskutilPartition {
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
struct DiskutilVolume {
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

#[derive(Debug, Clone)]
struct LsblkRow {
    name: String,
    path: String,
    size: Option<String>,
    device_type: String,
    mount_points: Vec<String>,
    model: Option<String>,
    pkname: Option<String>,
    fs_type: Option<String>,
    label: Option<String>,
    rm: bool,
    hotplug: bool,
    tran: Option<String>,
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

fn collect_mount_points(value: Option<&str>) -> Vec<String> {
    let mut mount_points = value
        .into_iter()
        .flat_map(|item| item.lines())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    mount_points.sort_by(|left, right| mount_path_sort_key(left).cmp(&mount_path_sort_key(right)));
    mount_points
}

fn device_path(identifier: &str) -> String {
    format!("/dev/{identifier}")
}

fn diskutil_nodes(
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
                    id: None,
                    name: String::from("Container"),
                    size: partition.size.map(format_bytes),
                    status: Some(String::from("container")),
                    detail: Some(String::from("APFS container")),
                    nodes: container_nodes,
                })
            } else if partition_is_visible(partition) {
                Some(DiskNode {
                    id: Some(device_path(&partition.device_identifier)),
                    name: partition_label(partition),
                    size: partition.size.map(format_bytes),
                    status: Some(String::from("unmounted")),
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

fn diskutil_volume_nodes(
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
                    id: Some(device_path(&volume.device_identifier)),
                    name: name.to_string(),
                    size: volume.capacity_in_use.or(volume.size).map(format_bytes),
                    status: Some(if mounted { "mounted" } else { "unmounted" }.to_string()),
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

fn lsblk_children(
    parent: &str,
    rows: &[LsblkRow],
    children: &std::collections::HashMap<String, Vec<usize>>,
) -> (Vec<DiskNode>, bool) {
    children
        .get(parent)
        .into_iter()
        .flat_map(|indexes| indexes.iter().copied())
        .map(|index| {
            let row = &rows[index];
            let (nodes, active) = lsblk_children(&row.name, rows, children);
            let is_swap = row
                .fs_type
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("swap"));
            let mounted_here = mounted(&row.mount_points) || is_swap;

            let node_status = if is_swap {
                "swap"
            } else if mounted(&row.mount_points) {
                "mounted"
            } else {
                "unmounted"
            };
            (
                DiskNode {
                    id: Some(row.path.clone()),
                    name: lsblk_label(row),
                    size: row.size.clone(),
                    status: Some(node_status.to_string()),
                    detail: node_detail(row, is_swap),
                    nodes,
                },
                active || mounted_here,
            )
        })
        .fold(
            (Vec::new(), false),
            |(mut nodes, active), (node, mounted)| {
                nodes.push(node);
                (nodes, active || mounted)
            },
        )
}

fn preferred_diskutil_name(nodes: &[DiskNode]) -> Option<String> {
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

fn parse_diskutil_apfs_roles(stdout: &str) -> Result<std::collections::HashMap<String, String>> {
    let parsed: DiskutilApfsList =
        from_bytes(stdout.as_bytes()).context("failed to parse diskutil apfs plist output")?;

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

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    if bytes < 1000 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }

    format!("{value:.1} {}", UNITS[unit])
}

fn lsblk_label(row: &LsblkRow) -> String {
    row.label
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            row.fs_type
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("swap"))
                .then(|| String::from("Swap"))
        })
        .unwrap_or_else(|| row.name.clone())
}

fn mounted(mount_points: &[String]) -> bool {
    mount_points.iter().any(|value| !value.trim().is_empty())
}

fn node_detail(row: &LsblkRow, is_swap: bool) -> Option<String> {
    if is_swap {
        return Some(String::from("swap"));
    }

    if mounted(&row.mount_points) {
        return Some(row.mount_points.join(" • "));
    }

    row.fs_type.clone().filter(|value| !value.trim().is_empty())
}

fn parse_lsblk_row(line: &str) -> Option<LsblkRow> {
    let pairs = parse_lsblk_pairs(line);
    let name = pairs.get("NAME")?.trim().to_string();

    Some(LsblkRow {
        path: pairs
            .get("PATH")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| device_path(&name)),
        size: pairs
            .get("SIZE")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        device_type: pairs.get("TYPE")?.trim().to_string(),
        mount_points: pairs
            .get("MOUNTPOINTS")
            .map(String::as_str)
            .map(|value| collect_mount_points(Some(value)))
            .unwrap_or_default(),
        model: pairs
            .get("MODEL")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        pkname: pairs
            .get("PKNAME")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        fs_type: pairs
            .get("FSTYPE")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        label: pairs
            .get("LABEL")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        rm: pairs
            .get("RM")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
        hotplug: pairs
            .get("HOTPLUG")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
        tran: pairs
            .get("TRAN")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        name,
    })
}

fn parse_lsblk_pairs(line: &str) -> std::collections::HashMap<String, String> {
    let mut pairs = std::collections::HashMap::new();
    let chars = line.chars().collect::<Vec<_>>();
    let mut index = 0;

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }

        let key_start = index;
        while index < chars.len() && chars[index] != '=' {
            index += 1;
        }

        if key_start == index || index >= chars.len() {
            break;
        }

        let key = chars[key_start..index].iter().collect::<String>();
        index += 1;

        if index >= chars.len() || chars[index] != '"' {
            continue;
        }

        index += 1;
        let mut value = String::new();

        while index < chars.len() {
            match chars[index] {
                '"' => {
                    index += 1;
                    break;
                }
                '\\' if index + 1 < chars.len() && chars[index + 1] == 'x' => {
                    if index + 3 < chars.len() {
                        let hex = chars[index + 2..=index + 3].iter().collect::<String>();
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            value.push(byte as char);
                            index += 4;
                            continue;
                        }
                    }

                    value.push(chars[index]);
                    index += 1;
                }
                '\\' if index + 1 < chars.len() => {
                    value.push(chars[index + 1]);
                    index += 2;
                }
                ch => {
                    value.push(ch);
                    index += 1;
                }
            }
        }

        pairs.insert(key, value.trim().to_string());
    }

    pairs
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

fn linux_disk_name(row: &LsblkRow) -> (String, Option<String>) {
    let model = row
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if let Some(model) = model {
        if let Some((name, suffix)) = split_bracket_suffix(model) {
            return (name, Some(suffix));
        }

        return (model.to_string(), None);
    }

    (row.name.clone(), None)
}

fn linux_disk_detail(
    row: &LsblkRow,
    extra_detail: Option<String>,
    nodes: &[DiskNode],
) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(location) = if row.hotplug || row.rm || row.tran.as_deref() == Some("usb") {
        Some("external")
    } else {
        Some("internal")
    } {
        parts.push(location.to_string());
    }

    if let Some(transport) = row
        .tran
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_ascii_uppercase())
    {
        parts.push(transport);
    }

    if let Some(detail) = extra_detail.filter(|value| !value.trim().is_empty()) {
        parts.push(detail);
    }

    if disk_nodes_have_mount(nodes, "/") {
        parts.push(String::from("system"));
    }

    (!parts.is_empty()).then(|| parts.join(" • "))
}

fn linux_disk_sort_key(disk: &Disk) -> (u8, u8, String) {
    let system_rank = if disk_nodes_have_mount(&disk.nodes, "/") {
        0
    } else {
        1
    };
    let external_rank = if disk
        .detail
        .as_deref()
        .is_some_and(|detail| detail.contains("external"))
    {
        1
    } else {
        0
    };

    (system_rank, external_rank, disk.id.clone())
}

fn disk_nodes_have_mount(nodes: &[DiskNode], mount: &str) -> bool {
    nodes.iter().any(|node| {
        node.detail
            .as_deref()
            .is_some_and(|detail| detail.split(", ").any(|part| part == mount))
            || disk_nodes_have_mount(&node.nodes, mount)
    })
}

fn split_bracket_suffix(value: &str) -> Option<(String, String)> {
    let (name, suffix) = value.rsplit_once(" [")?;
    suffix.ends_with(']').then(|| {
        (
            name.trim().to_string(),
            suffix.trim_end_matches(']').trim().to_string(),
        )
    })
}

fn mount_path_sort_key(path: &str) -> (usize, usize, &str) {
    let normalized = path.trim_matches('/');
    let depth = if normalized.is_empty() {
        0
    } else {
        normalized.split('/').count()
    };

    (depth, path.len(), path)
}
