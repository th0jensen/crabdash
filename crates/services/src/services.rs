use serde::Serialize;

#[derive(Clone, Debug, Default)]
pub struct MachineServices {
    pub docker: Vec<ServiceItem>,
    pub disks: Vec<ServiceItem>,
    pub systemd: Vec<ServiceItem>,
    pub docker_error: Option<String>,
    pub disks_error: Option<String>,
    pub systemd_error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ServiceKind {
    Docker,
    Disks,
    Systemd,
}

impl ServiceKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Disks => "disks",
            Self::Systemd => "systemd",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ServiceItem {
    pub id: String,
    pub name: String,
    pub kind: ServiceKind,
    pub status: String,
    pub error: Option<String>,
}

impl ServiceItem {
    pub fn convert_docker(stdout: String) -> Vec<ServiceItem> {
        stdout
            .lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');

                let id = parts.next()?.to_string();
                let name = parts.next()?.to_string();
                let state = parts.next()?.to_string();

                Some(ServiceItem {
                    id,
                    name,
                    kind: ServiceKind::Docker,
                    status: state,
                    error: None,
                })
            })
            .collect()
    }

    pub fn convert_diskutil(stdout: &str) -> Vec<ServiceItem> {
        stdout
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();

                if !trimmed.starts_with("/dev/") || !trimmed.ends_with(':') {
                    return None;
                }

                let id = trimmed
                    .trim_end_matches(':')
                    .split_whitespace()
                    .next()?
                    .to_string();
                let name = trimmed
                    .trim_end_matches(':')
                    .split_once('(')
                    .map(|(device, description)| {
                        let device = device.trim().trim_start_matches("/dev/");
                        let description = description.trim().trim_end_matches(')');
                        format!("{device} ({description})")
                    })
                    .unwrap_or_else(|| id.trim_start_matches("/dev/").to_string());

                Some(ServiceItem {
                    id,
                    name,
                    kind: ServiceKind::Disks,
                    status: String::from("healthy"),
                    error: None,
                })
            })
            .collect()
    }

    pub fn convert_lsblk(stdout: &str) -> Vec<ServiceItem> {
        stdout
            .lines()
            .filter_map(|line| {
                let mut pairs = std::collections::HashMap::new();
                let chars: Vec<char> = line.chars().collect();
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

                    let key: String = chars[key_start..index].iter().collect();
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
                                    let hex: String = chars[index + 2..=index + 3].iter().collect();
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

                if pairs.get("TYPE")?.trim() != "disk" {
                    return None;
                }

                let fallback_name = pairs.get("NAME")?.trim();
                let model = pairs
                    .get("MODEL")
                    .map(String::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::trim);
                let size = pairs
                    .get("SIZE")
                    .map(String::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::trim);

                let name = match (model, size) {
                    (Some(model), Some(size)) => format!("{model} ({size})"),
                    (Some(model), None) => model.to_string(),
                    (None, Some(size)) => format!("{fallback_name} ({size})"),
                    (None, None) => fallback_name.to_string(),
                };

                let id = pairs
                    .get("PATH")
                    .map(String::as_str)
                    .filter(|path| !path.trim().is_empty())
                    .map(|path| path.to_string())
                    .or_else(|| pairs.get("NAME").map(|name| format!("/dev/{name}")))?;

                let status = if pairs
                    .get("MOUNTPOINTS")
                    .map(String::as_str)
                    .map(str::trim)
                    .is_some_and(|mountpoints| !mountpoints.is_empty())
                {
                    String::from("mounted")
                } else {
                    String::from("healthy")
                };

                Some(ServiceItem {
                    id,
                    name,
                    kind: ServiceKind::Disks,
                    status,
                    error: None,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub output: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{ServiceItem, ServiceKind};

    #[test]
    fn convert_diskutil_extracts_top_level_disks() {
        let stdout = r#"
/dev/disk0 (internal, physical):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:      GUID_partition_scheme                        *251.0 GB   disk0

/dev/disk3 (synthesized):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:      APFS Container Scheme -                      +245.1 GB   disk3
"#;

        let disks = ServiceItem::convert_diskutil(stdout);

        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].id, "/dev/disk0");
        assert_eq!(disks[0].name, "disk0 (internal, physical)");
        assert_eq!(disks[0].status, "healthy");
        assert!(matches!(disks[0].kind, ServiceKind::Disks));
    }

    #[test]
    fn convert_lsblk_extracts_disk_rows() {
        let stdout = concat!(
            "NAME=\"sda\" PATH=\"/dev/sda\" SIZE=\"931.5G\" TYPE=\"disk\" MOUNTPOINTS=\"\" MODEL=\"Samsung SSD\" STATE=\"running\"\n",
            "NAME=\"nvme0n1\" PATH=\"/dev/nvme0n1\" SIZE=\"476.9G\" TYPE=\"disk\" MOUNTPOINTS=\"/\" MODEL=\"\" STATE=\"live\"\n",
            "NAME=\"loop0\" PATH=\"/dev/loop0\" SIZE=\"63.2M\" TYPE=\"loop\" MOUNTPOINTS=\"/snap/core20/2318\" MODEL=\"\" STATE=\"\"\n",
        );

        let disks = ServiceItem::convert_lsblk(stdout);

        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].id, "/dev/sda");
        assert_eq!(disks[0].name, "Samsung SSD (931.5G)");
        assert_eq!(disks[0].status, "healthy");
        assert_eq!(disks[1].id, "/dev/nvme0n1");
        assert_eq!(disks[1].name, "nvme0n1 (476.9G)");
        assert_eq!(disks[1].status, "mounted");
    }
}
