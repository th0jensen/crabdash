use crate::{commands::SystemInfo, remote_connection::RemoteConnection, store::MachineStore};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use services::{MachineServices, ServiceItem, disks::Disks, docker::Docker};
use std::process::Command;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Machine {
    pub id: String,
    pub system_info: SystemInfo,
    pub kind: MachineKind,
    pub remote: Option<RemoteConnection>,
    pub docker_path: Option<String>,
    #[serde(skip)]
    pub services: MachineServices,
}

impl Machine {
    pub fn new_remote(user: &str, host: &str, password: &str) -> Result<Self> {
        let mut rc = RemoteConnection::new_connection(user, host, password)?;
        let system_info = SystemInfo::remote(&mut rc)?;

        Ok(Self {
            id: format!("{user}@{host}"),
            kind: MachineKind::get_kind(&system_info),
            system_info,
            remote: Some(rc),
            docker_path: None,
            services: MachineServices::default(),
        })
    }

    pub fn run(&mut self, cmd: &str, args: Option<&[&str]>) -> Result<String> {
        match &mut self.remote {
            Some(rc) => {
                let (stdout, exit_status) = rc.run_ssh_command(cmd, args)?;
                if exit_status != 0 {
                    let message = if stdout.trim().is_empty() {
                        format!("{cmd} exited with status {exit_status}")
                    } else {
                        stdout.trim().to_string()
                    };
                    eprintln!(
                        "Remote command failed: cmd={cmd} args={:?} status={} output={}",
                        args, exit_status, message
                    );
                    return Err(anyhow!(message));
                }
                Ok(stdout)
            }
            None => {
                let result = Command::new(cmd).args(args.unwrap_or(&[])).output()?;
                if !result.status.success() {
                    let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
                    let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
                    let message = if !stderr.is_empty() {
                        stderr
                    } else if !stdout.is_empty() {
                        stdout
                    } else {
                        format!("{cmd} exited with status {}", result.status)
                    };
                    eprintln!(
                        "Local command failed: cmd={cmd} args={:?} status={} stderr={} stdout={}",
                        args,
                        result.status,
                        String::from_utf8_lossy(&result.stderr).trim(),
                        String::from_utf8_lossy(&result.stdout).trim()
                    );
                    return Err(anyhow!(message));
                }
                Ok(String::from_utf8_lossy(&result.stdout).to_string())
            }
        }
    }

    pub fn background_clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            system_info: self.system_info.clone(),
            kind: self.kind,
            remote: self.remote.clone(),
            docker_path: self.docker_path.clone(),
            services: MachineServices::default(),
        }
    }

    pub fn has_active_connection(&self) -> bool {
        self.remote
            .as_ref()
            .map_or(true, |remote| remote.has_active_session())
    }

    pub fn sync_system_info(&mut self) -> Result<bool> {
        let system_info = match &mut self.remote {
            Some(rc) => SystemInfo::remote(rc)?,
            None => SystemInfo::local()?,
        };

        if self.system_info == system_info {
            return Ok(false);
        }

        self.kind = MachineKind::get_kind(&system_info);
        self.system_info = system_info;
        MachineStore::save_machine(self.clone()).ok();

        Ok(true)
    }
}

impl Docker for Machine {
    fn find_docker(&mut self) -> String {
        if let Some(path) = &self.docker_path {
            return path.clone();
        }

        const CANDIDATES: &[&str] = &[
            "/opt/homebrew/bin/docker",
            "/usr/local/bin/docker",
            "/usr/bin/docker",
        ];

        let path = CANDIDATES
            .iter()
            .copied()
            .find(|p| self.run("test", Some(&["-f", p])).is_ok())
            .map(|p| p.to_string())
            .unwrap_or_else(|| String::from("docker"));
        self.docker_path = Some(path.clone());
        MachineStore::save_machine(self.clone()).ok();
        path
    }

    fn list_docker(&mut self) -> Result<Vec<ServiceItem>> {
        let docker = self.find_docker();
        let stdout = self.run(
            &docker,
            Some(&["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.State}}"]),
        )?;
        Ok(ServiceItem::convert_docker(stdout))
    }

    fn container_action(&mut self, id: &str, action: &str) -> Result<String> {
        let args = [action, id];
        let docker = self.find_docker();
        let stdout = self.run(&docker, Some(&args))?;
        if stdout.trim() != id {
            bail!(stdout)
        }
        Ok(stdout)
    }

    fn container_logs(&mut self, _id: &str) -> Result<String> {
        todo!()
    }
}

impl Disks for Machine {
    fn list_disks(&mut self) -> Result<Vec<ServiceItem>> {
        match self.kind {
            MachineKind::MacOS => {
                let stdout = self.run("diskutil", Some(&["list"]))?;
                Ok(ServiceItem::convert_diskutil(&stdout))
            }
            MachineKind::Linux => {
                let stdout = self.run(
                    "lsblk",
                    Some(&["-P", "-o", "NAME,PATH,SIZE,TYPE,MOUNTPOINTS,MODEL"]),
                )?;
                Ok(ServiceItem::convert_lsblk(&stdout))
            }
            MachineKind::Unknown => bail!("System does not yet support the disks feature"),
        }
    }
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
