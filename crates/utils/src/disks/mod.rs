//! Disk model and platform-independent formatting.
pub mod linux;
pub mod macos;
use anyhow::Result;
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
    pub name: String,
    pub size: Option<String>,
    pub detail: Option<String>,
    pub nodes: Vec<DiskNode>,
}

impl Disk {
    pub fn is_healthy(&self) -> bool {
        matches!(self.status.as_str(), "healthy" | "mounted" | "swap")
    }
}
pub trait Disks {
    /// Lists all disks connected to the machine
    ///
    /// # Returns
    /// * `Ok(Vec<Disk>)`: The disks connected to the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_disks(&mut self) -> impl Future<Output = Result<Vec<Disk>>>;
}
pub fn collect_mount_points(value: Option<&str>) -> Vec<String> {
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

pub fn format_bytes(bytes: u64) -> String {
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

pub fn mounted(mount_points: &[String]) -> bool {
    mount_points.iter().any(|value| !value.trim().is_empty())
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

pub fn device_path(identifier: &str) -> String {
    format!("/dev/{identifier}")
}
