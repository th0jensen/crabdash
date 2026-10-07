//! Whole-device byte counters and measured per-second disk I/O.
pub(super) mod linux;
pub(super) mod macos;
pub(super) mod windows;
#[derive(Clone, Debug)]
pub struct DiskCounter {
    pub id: String,
    pub read_bytes: u64,
    pub written_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct DiskUsage {
    pub id: String,
    pub read_bytes_per_second: Option<f64>,
    pub written_bytes_per_second: Option<f64>,
}
