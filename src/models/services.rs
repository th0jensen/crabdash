use std::error::Error;

use serde::Serialize;

#[derive(Debug, Default)]
pub struct MachineServices {
    pub docker: Vec<ServiceItem>,
    pub disks: Vec<ServiceItem>,
    pub systemd: Vec<ServiceItem>,
    pub docker_error: Option<Box<dyn Error>>,
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

impl ServiceItem {
    pub fn convert_docker(stdout: String) -> Result<Vec<ServiceItem>, Box<dyn Error>> {
        let containers = stdout
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
            .collect();

        Ok(containers)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub output: Option<String>,
}
