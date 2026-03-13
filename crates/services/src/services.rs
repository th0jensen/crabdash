use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Default)]
pub struct MachineServices {
    pub docker: Vec<ServiceItem>,
    pub disks: Vec<ServiceItem>,
    pub systemd: Vec<ServiceItem>,
    pub docker_error: Option<anyhow::Error>,
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
}

pub trait Docker {
    const DOCKER_CMD: &str;
    /// Lists all Docker containers on the machine
    ///
    /// # Returns
    /// * `Ok(Vec<ServiceItem>)`: The containers on the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_docker(&mut self) -> Result<Vec<ServiceItem>>;
    /// Runs an action on a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    /// * `action`: The action to perform (start/stop/restart)
    ///
    /// # Returns
    /// * `Ok(String)`: The ID of the container is returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn container_action(&mut self, id: &str, action: &str) -> Result<String>;
    /// Gets the logs of a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    ///
    /// # Returns
    /// * `Ok(_)`: The container logs are returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn container_logs(&mut self, id: &str) -> Result<String>;
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
