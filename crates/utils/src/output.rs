use anyhow::Result;
use plist::from_bytes;

use crate::{container::Container, disks::*, service_item::ServiceItem};

#[derive(Debug, Clone)]
pub struct Output(pub Vec<u8>);

impl Output {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }

    pub fn parse_container(&self) -> Vec<Container> {
        self.lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');

                let id = parts.next()?.to_string();
                let name = parts.next()?.to_string();
                let state = parts.next()?.to_string();

                Some(Container {
                    id,
                    name,
                    status: state,
                    error: None,
                })
            })
            .collect()
    }

    pub fn parse_service_item(&self) -> Vec<ServiceItem> {
        self.lines()
            .skip(1)
            .filter_map(|line| {
                let mut parts = line.split('\t');

                let pid = parts.next()?.to_string();
                let status = parts.next()?.to_string();
                let name = parts.next()?.to_string();

                if name.contains("●") {
                    return None;
                }

                Some(ServiceItem {
                    id: pid,
                    name: name,
                    status,
                    error: None,
                })
            })
            .collect()
    }

    pub fn parse_diskutil(self, apfs_stdout: Option<Self>) -> Result<Vec<Disk>> {
        let parsed: DiskutilList = from_bytes(&self.0)?;
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
                    detail: entry.os_internal.and_then(|is_internal: bool| {
                        is_internal.then_some(String::from("internal"))
                    }),
                    nodes,
                }
            })
            .collect())
    }

    pub fn parse_lsblk(self) -> Vec<Disk> {
        let rows = self.lines().filter_map(parse_lsblk_row).collect::<Vec<_>>();
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

impl Default for Output {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<u8>> for Output {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl From<String> for Output {
    fn from(s: String) -> Self {
        Self(s.into_bytes())
    }
}

impl From<Output> for String {
    fn from(o: Output) -> Self {
        String::from_utf8_lossy(&o.0).trim().to_string()
    }
}

impl From<Output> for Vec<u8> {
    fn from(o: Output) -> Self {
        o.0
    }
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}

impl AsRef<[u8]> for Output {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::ops::Deref for Output {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}
