//! Interface byte counters and measured per-second traffic.
pub(super) mod linux;
pub(super) mod macos;
pub(super) mod windows;
#[derive(Clone, Debug)]
pub struct NetworkCounter {
    pub id: String,
    pub received_bytes: u64,
    pub sent_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct NetworkUsage {
    pub id: String,
    pub received_bytes_per_second: Option<f64>,
    pub sent_bytes_per_second: Option<f64>,
}
