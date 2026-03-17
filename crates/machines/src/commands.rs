use crate::remote_connection::RemoteConnection;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use services::ServiceItem;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemInfo {
    pub machine_name: String,
    pub os_version: String,
    pub arch: String,
}

impl SystemInfo {
    pub fn local() -> Result<Self> {
        Ok(Self {
            machine_name: Self::run_uname(&["-n"])?,
            os_version: Self::run_uname(&["-sr"])?,
            arch: Self::run_uname(&["-m"])?,
        })
    }

    pub fn remote(rc: &mut RemoteConnection) -> Result<Self> {
        let cmd = "uname";
        let (machine_name, _) = rc.run_ssh_command(cmd, Some(&["-n"]))?;
        let (os_version, _) = rc.run_ssh_command(cmd, Some(&["-sr"]))?;
        let (arch, _) = rc.run_ssh_command(cmd, Some(&["-m"]))?;

        Ok(Self {
            machine_name: machine_name.trim().to_string(),
            os_version: os_version.trim().to_string(),
            arch: arch.trim().to_string(),
        })
    }

    fn run_uname(args: &[&str]) -> Result<String> {
        let result = Command::new("uname").args(args).output()?;

        if !result.status.success() {
            let message = String::from_utf8_lossy(&result.stderr).trim().to_string();

            bail!(if message.is_empty() {
                "uname silently failed to run."
            } else {
                "uname failed: {message}"
            })
        }

        let output = String::from_utf8_lossy(&result.stdout).trim().to_string();

        Ok(output)
    }
}

pub async fn docker_action(_container: String, _action: String) -> Result<String> {
    todo!()
}

pub async fn list_local_disks() -> Result<Vec<ServiceItem>> {
    todo!()
}

pub async fn run_ssh_command(_host: String, _command: String) -> Result<String> {
    todo!()
}

pub async fn list_remote_systemd(_host: String) -> Result<Vec<ServiceItem>> {
    todo!()
}

pub async fn systemd_action(_host: String, _unit: String, _action: String) -> Result<String> {
    todo!()
}
