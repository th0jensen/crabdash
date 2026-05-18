use anyhow::Result;
use serde::Serialize;
use utils::{container::Container, disks::Disk, output::Output, service_item::ServiceItem};

#[derive(Clone, Debug, Default)]
pub struct MachineServices {
    pub docker: Vec<Container>,
    pub disks: Vec<Disk>,
    pub systemd: Vec<ServiceItem>,
    pub docker_error: Option<String>,
    pub disks_error: Option<String>,
    pub systemd_error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ServiceFilter {
    #[default]
    Total,
    Running,
}

pub trait Services {
    /// Lists all services running on the machine
    ///
    /// # Returns
    /// * `Ok(Vec<Disk>)`: The disks connected to the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_services(&mut self) -> impl Future<Output = Result<Vec<ServiceItem>>>;

    /// Returns recent logs for a system service.
    fn service_logs(&mut self, service: &str) -> impl Future<Output = Result<Output>>;
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub output: Option<String>,
}
