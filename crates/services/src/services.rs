use anyhow::Result;
use lucide_icons::Icon;
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

    /// Performs an action on a system service.
    fn service_action(
        &mut self,
        service: &str,
        action: ServiceAction,
    ) -> impl Future<Output = Result<Output>>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
}

impl ServiceAction {
    pub fn command(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::Start => Icon::Play,
            Self::Stop => Icon::X,
            Self::Restart => Icon::RefreshCw,
        }
    }

    pub fn pending_label(self) -> &'static str {
        match self {
            Self::Start => "Starting",
            Self::Stop => "Stopping",
            Self::Restart => "Restarting",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub output: Option<String>,
}
