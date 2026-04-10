use anyhow::Result;
use serde::Serialize;

use crate::{disks::Disk, docker::Container};

#[derive(Clone, Debug, Default)]
pub struct MachineServices {
    pub docker: Vec<Container>,
    pub disks: Vec<Disk>,
    pub systemd: Vec<ServiceItem>,
    pub docker_error: Option<String>,
    pub disks_error: Option<String>,
    pub systemd_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ServiceItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ServiceFilter {
    #[default]
    Total,
    Running,
}

impl ServiceItem {
    pub fn is_running(&self) -> bool {
        if self.status.contains("0") || !self.status.to_ascii_lowercase().contains("inactive") {
            return true;
        }
        false
    }

    pub fn parse_output(stdout: String) -> Vec<ServiceItem> {
        stdout
            .lines()
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
}

pub trait Services {
    /// Lists all services running on the machine
    ///
    /// # Returns
    /// * `Ok(Vec<Disk>)`: The disks connected to the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_services(&mut self) -> impl Future<Output = Result<Vec<ServiceItem>>>;
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub output: Option<String>,
}
