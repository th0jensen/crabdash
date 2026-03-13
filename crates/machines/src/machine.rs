use crate::{commands::SystemInfo, remote_connection::RemoteConnection};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use services::{MachineServices, ServiceItem, docker::Docker};
use std::process::Command;

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
    pub fn new_remote(user: &str, host: &str, password: &str) -> Result<Self> {
        let mut remote = RemoteConnection::new_connection(user, host, password)?;
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
}

impl Docker for Machine {
    const DOCKER_CMD: &str = "docker";

    fn list_docker(&mut self) -> Result<Vec<ServiceItem>> {
        let args = ["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.State}}"];
        let stdout = self.run(Self::DOCKER_CMD, Some(&args))?;
        Ok(ServiceItem::convert_docker(stdout))
    }

    fn container_action(&mut self, id: &str, action: &str) -> Result<String> {
        let args = [action, id];
        let stdout = self.run(Self::DOCKER_CMD, Some(&args))?;
        if stdout.trim() != id {
            bail!(stdout)
        }
        Ok(stdout)
    }

    fn container_logs(&mut self, id: &str) -> Result<String> {
        todo!()
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
