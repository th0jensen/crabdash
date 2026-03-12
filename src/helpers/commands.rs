use crate::{
    helpers::remote_connection::RemoteConnection,
    models::{
        machine::Machine,
        services::{ServiceItem, ServiceKind},
    },
};
use serde::{Deserialize, Serialize};
use std::{error::Error, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub machine_name: String,
    pub os_version: String,
    pub arch: String,
}

impl SystemInfo {
    pub fn local() -> Result<Self, String> {
        Ok(Self {
            machine_name: Self::run_uname(&["-n"])?,
            os_version: Self::run_uname(&["-sr"])?,
            arch: Self::run_uname(&["-m"])?,
        })
    }

    pub fn remote(rc: &mut RemoteConnection) -> Result<Self, Box<dyn Error>> {
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

    fn run_uname(args: &[&str]) -> Result<String, String> {
        let result = Command::new("uname")
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;

        if !result.status.success() {
            let message = String::from_utf8_lossy(&result.stderr).trim().to_string();

            return Err(if message.is_empty() {
                "uname silently failed to run.".to_string()
            } else {
                format!("uname failed: {message}")
            });
        }

        let output = String::from_utf8_lossy(&result.stdout).trim().to_string();

        Ok(output)
    }
}

pub fn list_docker(mc: &mut Machine) -> Result<Vec<ServiceItem>, Box<dyn Error>> {
    let args = ["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.State}}"];
    let stdout = match &mut mc.remote {
        Some(rc) => {
            let (stdout, _) = rc.run_ssh_command("docker", Some(&args))?;
            stdout
        }
        None => {
            let result = Command::new("docker")
                .args(args)
                .output()
                .map_err(|e| format!("Failed to run docker: {e}"))?;
            String::from_utf8_lossy(&result.stdout).to_string()
        }
    };

    let containers = ServiceItem::convert_docker(stdout)?;
    return Ok(containers);
}

pub async fn docker_action(_container: String, _action: String) -> Result<String, String> {
    todo!()
}

pub async fn list_local_disks() -> Result<Vec<ServiceItem>, String> {
    todo!()
}

pub async fn run_ssh_command(_host: String, _command: String) -> Result<String, String> {
    todo!()
}

pub async fn list_remote_systemd(_host: String) -> Result<Vec<ServiceItem>, String> {
    todo!()
}

pub async fn systemd_action(
    _host: String,
    _unit: String,
    _action: String,
) -> Result<String, String> {
    todo!()
}
