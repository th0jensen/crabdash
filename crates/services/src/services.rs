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

impl ServiceItem {
    pub fn parse_output_mac(stdout: String) -> Vec<ServiceItem> {
        stdout
            .lines()
            .skip(1)
            .filter_map(|line| {
                let mut parts = line.split('\t');

                let pid = parts.next()?.to_string();
                let status = parts.next()?.to_string();
                let label = parts.next()?.to_string();

                Some(ServiceItem {
                    id: pid,
                    name: label,
                    status,
                    error: None,
                })
            })
            .collect()
    }

    pub fn parse_output_linux(stdout: String) -> Vec<ServiceItem> {
        stdout
            .lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');

                let name = parts.next()?;
                let status = parts.next()?;
                let pid = parts.next()?;

                Some(ServiceItem {
                    id: pid.to_string(),
                    name: name.to_string(),
                    status: status.to_string(),
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
    fn list_services(&mut self) -> Result<Vec<ServiceItem>>;
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub output: Option<String>,
}
