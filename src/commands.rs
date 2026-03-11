use crate::models::{ServiceItem, ServiceKind};
use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
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

pub fn list_local_docker() -> Result<Vec<ServiceItem>, String> {
    let result = Command::new("docker")
        .arg("ps")
        .arg("-a")
        .arg("--format")
        .arg("{{.ID}}\t{{.Names}}\t{{.State}}")
        .output()
        .map_err(|e| format!("Docker not installed: {e}").to_string())?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let message = stderr.trim();

        return Err(if message.is_empty() {
            "Docker returned a non-zero exit status.".to_string()
        } else {
            format!("Docker command failed: {message}")
        });
    }

    let stdout = String::from_utf8_lossy(&result.stdout);

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
