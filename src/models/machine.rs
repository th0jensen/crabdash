use crate::{
    helpers::{commands::SystemInfo, remote_connection::RemoteConnection},
    models::services::MachineServices,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

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
    pub fn new(id: String, kind: MachineKind) -> Result<Self> {
        Ok(Self {
            id,
            system_info: SystemInfo::local()?,
            kind,
            remote: None,
            services: MachineServices::default(),
        })
    }

    pub fn new_remote(user: String, host: String, password: String) -> Result<Self> {
        let mut remote = RemoteConnection::new_connection(user.clone(), host.clone(), password)?;
        let system_info = SystemInfo::remote(&mut remote)?;
        remote.store_password()?;

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
    pub fn load() -> Result<Self> {
        let path = machines_file_path()?;

        if !path.exists() {
            let store = MachineStore::default();
            store.save()?;
            return Ok(store);
        };

        let contents = fs::read_to_string(&path)?;
        let store = serde_json::from_str(&contents)?;
        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let path = machines_file_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn add_machine(&mut self, machine: Machine) -> Result<usize> {
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
    ) -> Result<usize> {
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

fn machines_file_path() -> Result<PathBuf> {
    let mut path = dirs::data_local_dir().context("Could not determine local data directory")?;

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
