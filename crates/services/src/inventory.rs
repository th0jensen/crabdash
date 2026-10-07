//! Cached feature results for a machine; independent of a particular service manager.
use utils::{container::Container, disks::Disk, services::ServiceItem};
#[derive(Clone, Debug, Default)]
pub struct MachineServices {
    pub docker: Vec<Container>,
    pub disks: Vec<Disk>,
    pub systemd: Vec<ServiceItem>,
    pub docker_error: Option<String>,
    pub docker_not_installed: bool,
    pub disks_error: Option<String>,
    pub systemd_error: Option<String>,
}
