use super::{Disk, DiskNode, collect_mount_points, device_path, mounted};
use crate::output::Output;
#[derive(Debug, Clone)]
pub struct LsblkRow {
    pub name: String,
    pub path: String,
    pub size: Option<String>,
    pub device_type: String,
    pub mount_points: Vec<String>,
    pub model: Option<String>,
    pub pkname: Option<String>,
    pub fs_type: Option<String>,
    pub label: Option<String>,
    pub rm: bool,
    pub hotplug: bool,
    pub tran: Option<String>,
}

pub fn lsblk_children(
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

            (
                DiskNode {
                    name: lsblk_label(row),
                    size: row.size.clone(),
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

pub fn lsblk_label(row: &LsblkRow) -> String {
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

fn node_detail(row: &LsblkRow, is_swap: bool) -> Option<String> {
    if is_swap {
        return Some(String::from("swap"));
    }

    if mounted(&row.mount_points) {
        return Some(row.mount_points.join(" • "));
    }

    row.fs_type.clone().filter(|value| !value.trim().is_empty())
}

pub fn parse_lsblk_row(line: &str) -> Option<LsblkRow> {
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

pub fn linux_disk_name(row: &LsblkRow) -> (String, Option<String>) {
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

pub fn linux_disk_detail(
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

pub fn linux_disk_sort_key(disk: &Disk) -> (u8, u8, String) {
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

pub fn parse(output: Output) -> Vec<Disk> {
    let rows = output
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mounted_partition_tree_without_losing_device_metadata() {
        let output = Output::from(concat!(
            "NAME=\"vda\" PATH=\"/dev/vda\" SIZE=\"64G\" TYPE=\"disk\" MODEL=\"Virtual Disk\" RM=\"0\" HOTPLUG=\"0\" TRAN=\"virtio\"\n",
            "NAME=\"vda1\" PATH=\"/dev/vda1\" SIZE=\"60G\" TYPE=\"part\" PKNAME=\"vda\" FSTYPE=\"ext4\" LABEL=\"Root\" MOUNTPOINTS=\"/\"\n"
        ).to_string());
        let disks = parse(output);
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].id, "/dev/vda");
        assert_eq!(disks[0].name, "Virtual Disk");
        assert_eq!(disks[0].status, "mounted");
        assert!(disks[0].detail.as_deref().unwrap().contains("system"));
        assert_eq!(disks[0].nodes[0].name, "Root");
        assert_eq!(disks[0].nodes[0].detail.as_deref(), Some("/"));
    }
}
