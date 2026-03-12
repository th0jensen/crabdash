use std::{fs, path::PathBuf};

use crate::{
    helpers::{commands::SystemInfo, remote_connection::RemoteConnection},
    models::services::MachineServices,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Machine {
    pub id: String,
    pub system_info: SystemInfo,
    pub kind: MachineKind,
    pub remote: Option<RemoteConnection>,
    #[serde(skip)]
    pub services: MachineServices,
}

impl Machine {
    pub fn new(id: String, kind: MachineKind) -> Result<Self, String> {
        Ok(Self {
            id,
            system_info: SystemInfo::local()?,
            kind,
            remote: None,
            services: MachineServices::default(),
        })
    }

    pub fn new_remote(user: String, host: String, password: String) -> Result<Self, String> {
        let mut remote = RemoteConnection::new_connection(user.clone(), host.clone(), password)
            .map_err(|e| e.to_string())?;
        let system_info = SystemInfo::remote(&mut remote).map_err(|e| e.to_string())?;
        remote.store_password().map_err(|e| e.to_string())?;

        Ok(Self {
            id: format!("{user}@{host}"),
            kind: MachineKind::get_kind(&system_info),
            system_info,
            remote: Some(remote),
            services: MachineServices::default(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MachineStore {
    pub machines: Vec<Machine>,
}

impl MachineStore {
    pub fn load() -> Result<Self, String> {
        let path = machines_file_path()?;

        if !path.exists() {
            let store = MachineStore::default();
            Self::save(&store)
                .map_err(|e| format!("Failed to save MachineStore: {e}").to_string())?;
            return Ok(store);
        };

        let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&contents).map_err(|e| e.to_string())
    }

    pub fn save(&self) -> Result<(), String> {
        let path = machines_file_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn add_machine(&mut self, machine: Machine) -> Result<usize, String> {
        self.machines.push(machine);
        let index = self.machines.len() - 1;

        if let Err(error) = self.save() {
            self.machines.pop();
            return Err(error);
        }

        Ok(index)
    }

    pub fn add_remote_machine(
        &mut self,
        user: String,
        host: String,
        password: String,
    ) -> Result<usize, String> {
        let machine = Machine::new_remote(user, host, password)?;
        self.add_machine(machine)
    }
}

impl Default for MachineStore {
    fn default() -> Self {
        let sys = SystemInfo::local().expect("Failed to retrieve SystemInfo");
        let kind = MachineKind::get_kind(&sys);
        MachineStore {
            machines: vec![Machine {
                id: "localhost".to_string(),
                system_info: sys,
                kind,
                remote: None,
                services: MachineServices::default(),
            }],
        }
    }
}

fn machines_file_path() -> Result<PathBuf, String> {
    let mut path = dirs::data_local_dir()
        .ok_or_else(|| format!("Could not determine local data directory").to_string())?;

    path.push("com.thojensen.crabdash");
    path.push("machines.json");

    Ok(path)
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub enum MachineKind {
    MacOS,
    Linux,
    #[default]
    Unknown,
}

impl MachineKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::MacOS => "macOS",
            Self::Linux => "Linux",
            Self::Unknown => "Unknown",
        }
    }

    pub fn get_kind(machine: &SystemInfo) -> Self {
        match &machine.os_version {
            s if s.contains("Darwin") => Self::MacOS,
            s if s.contains("Linux") => Self::Linux,
            _ => Self::default(),
        }
    }
}
