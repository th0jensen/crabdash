use crate::commands::SystemInfo;
use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
pub struct Machine {
    pub id: String,
    pub system_info: SystemInfo,
    pub kind: MachineKind,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum MachineKind {
    MacOS,
    Linux,
}

impl MachineKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::MacOS => "macOS",
            Self::Linux => "Linux",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub output: Option<String>,
}
