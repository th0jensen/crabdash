//! Driver telemetry is optional; device discovery never invents utilization.
pub(super) mod linux;
pub(super) mod macos;
pub(super) mod windows;
#[derive(Clone, Debug)]
pub struct GpuSample {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub driver: Option<String>,
    pub busy_percent: Option<f64>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub temperature_celsius: Option<f64>,
}
