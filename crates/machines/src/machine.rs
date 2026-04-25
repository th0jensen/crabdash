use crate::{
    remote_connection::{AuthMethod, RemoteConnection},
    store::MachineStore,
};
use anyhow::{Result, anyhow, bail};
use indoc::indoc;
use serde::{Deserialize, Serialize};
use services::{MachineServices, Services, docker::Docker};
use smol::process::Command;
use utils::{
    args,
    args::Args,
    container::Container,
    disks::{Disk, Disks},
    output::Output,
    service_item::ServiceItem,
};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Machine {
    pub uuid: Uuid,
    pub id: String,
    pub system_info: SystemInfo,
    pub kind: MachineKind,
    pub remote: Option<RemoteConnection>,
    pub docker_path: Option<String>,
    #[serde(skip)]
    pub services: MachineServices,
}

impl Machine {
    /// Creates a new [`Machine`] connected to a remote host via SSH.
    ///
    /// Establishes an SSH connection and queries the remote system for its
    /// information, which is used to determine the machine kind.
    ///
    /// # Arguments
    /// * `user`: The SSH username
    /// * `host`: The hostname or IP address to connect to
    /// * `auth`: SSH authentication details
    ///
    /// # Returns
    /// * `Ok(Machine)`: A fully initialised machine with an active SSH connection
    /// * `Err(anyhow::Error)`: If the SSH connection fails, or if querying remote
    ///   system information fails
    pub async fn new_remote(user: &str, host: &str, auth: AuthMethod) -> Result<Self> {
        let rc = RemoteConnection::new_connection(user, host, auth).await?;
        let mut machine = Self {
            id: format!("{user}@{host}"),
            remote: Some(rc),
            ..Self::default()
        };

        machine.system_info = machine.get_system_info().await?;
        machine.kind = MachineKind::get_kind(&machine);
        Ok(machine)
    }
    /// Runs a command either locally or on the configured remote machine via SSH,
    /// depending on whether a remote connection is active.
    ///
    /// On failure, diagnostic output is written to stderr.
    ///
    /// # Arguments
    /// * `cmd`: The program to execute
    /// * `args`: Arguments to pass to the program. `None` is equivalent to `Some(&[])`.
    ///
    /// # Returns
    /// * `Ok(String)`: Captured stdout from the command
    /// * `Err(anyhow::Error)`: If the command exits with a non-zero status, or if
    ///   spawning/communication fails. The error message prefers stderr over stdout,
    ///   falling back to a generic exit status message if both are empty.
    pub async fn run(&mut self, cmd: &str, args: &Args) -> Result<Output> {
        match &mut self.remote {
            Some(rc) => {
                let stdout = rc.run_ssh_command(cmd, args).await?;
                Ok(stdout)
            }
            None => {
                let result = Command::new(cmd).args(args).output().await?;
                if !result.status.success() {
                    let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
                    let message = if !stderr.is_empty() {
                        stderr
                    } else {
                        format!("{cmd} exited with status {}", result.status)
                    };
                    tracing::error!(
                        cmd = %cmd,
                        args = ?args,
                        status = %result.status,
                        stderr = %String::from_utf8_lossy(&result.stderr).trim(),
                        stdout = %String::from_utf8_lossy(&result.stdout).trim(),
                        "Local command failed"
                    );
                    return Err(anyhow!(message));
                }
                Ok(Output::from(result.stdout))
            }
        }
    }
    /// Returns whether the machine has an active connection.
    ///
    /// For remote machines, reads the synchronously-maintained `connected` flag
    /// on the underlying [`RemoteConnection`]. For local machines, always `true`.
    pub fn connected(&self) -> bool {
        match self.remote.as_ref() {
            Some(rc) => rc.connected(),
            None => true,
        }
    }
    /// Refreshes system information from the machine and updates internal state
    /// if it has changed.
    ///
    /// If the newly retrieved info differs from the current state, the machine
    /// kind is re-evaluated and the updated machine is persisted via
    /// [`MachineStore::save_machine`]. Persistence failures are silently ignored.
    ///
    /// # Returns
    /// * `Ok(true)`: System info has changed and state was updated
    /// * `Ok(false)`: System info is unchanged
    /// * `Err(anyhow::Error)`: If querying system info fails
    pub async fn sync_system_info(&mut self) -> Result<bool> {
        let system_info = self.get_system_info().await?;

        if self.system_info == system_info {
            return Ok(false);
        }

        self.kind = MachineKind::get_kind_from_info(&system_info);
        self.system_info = system_info;

        MachineStore::update_machine(self.clone()).await?;
        Ok(true)
    }

    async fn get_system_info(&mut self) -> Result<SystemInfo> {
        let cmd = "uname";
        let (machine_name, os_version, arch) = (
            self.run(cmd, &args!["-n"]).await?.into(),
            self.run(cmd, &args!["-sr"]).await?.into(),
            self.run(cmd, &args!["-m"]).await?.into(),
        );
        Ok(SystemInfo {
            machine_name,
            os_version,
            arch,
        })
    }
}

impl Docker for Machine {
    async fn find_docker(&mut self) -> String {
        if let Some(path) = &self.docker_path {
            return path.clone();
        }

        const CANDIDATES: &[&str] = &[
            "/opt/homebrew/bin/docker",
            "/usr/local/bin/docker",
            "/usr/bin/docker",
        ];

        let path = {
            let mut found = None;
            for p in CANDIDATES.iter().copied() {
                if self.run("test", &args!["-f", p]).await.is_ok() {
                    found = Some(p.to_string());
                    break;
                }
            }
            found.unwrap_or_else(|| String::from("docker"))
        };

        self.docker_path = Some(path.clone());
        MachineStore::update_machine(self.clone()).await.ok();
        path
    }

    async fn list_docker(&mut self) -> Result<Vec<Container>> {
        let args = args!["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.State}}"];
        let docker = self.find_docker().await;
        Ok(self.run(&docker, &args).await?.parse_container())
    }

    async fn container_action(&mut self, id: &str, action: &str) -> Result<Output> {
        let args = args![action, id];
        let docker = self.find_docker().await;
        Ok(self.run(&docker, &args).await?)
    }

    async fn run_container(&mut self, args: &Args) -> Result<Output> {
        let docker = self.find_docker().await;
        Ok(self.run(&docker, &args).await?)
    }

    async fn remove_container(&mut self, id: &str) -> Result<Output> {
        let docker = self.find_docker().await;
        Ok(self.run(&docker, &args!["rm", id]).await?)
    }

    async fn container_logs(&mut self, id: &str) -> Result<Output> {
        let docker = self.find_docker().await;
        Ok(self.run(&docker, &args!["logs", id]).await?)
    }
}

impl Services for Machine {
    async fn list_services(&mut self) -> Result<Vec<ServiceItem>> {
        let (command, args): (&str, Args) = match self.kind {
            MachineKind::MacOS => ("launchctl", args!["list"]),
            MachineKind::Linux => (
                "sh",
                args![
                    "-c",
                    indoc! {r#"
                        systemctl list-units --type=service --all --no-legend --no-pager \
                        | awk '{print $1}' \
                        | while read -r unit; do
                            status=$(systemctl show -p ActiveState --value "$unit")
                            pid=$(systemctl show -p MainPID --value "$unit")
                            printf "%s\t%s\t%s\n" "$pid" "$status" "$unit"
                        done
                    "#}
                ],
            ),
            _ => bail!("System does not yet support the services feature"),
        };

        Ok(self.run(command, &args).await?.parse_service_item())
    }
}

impl Disks for Machine {
    async fn list_disks(&mut self) -> Result<Vec<Disk>> {
        match self.kind {
            MachineKind::MacOS => {
                let list_stdout = self.run("diskutil", &args!["list", "-plist"]).await?;
                let apfs_stdout = self
                    .run("diskutil", &args!["apfs", "list", "-plist"])
                    .await
                    .ok();
                let mut disks = list_stdout.parse_diskutil(apfs_stdout)?;

                for disk in &mut disks {
                    let identifier = disk.id.trim_start_matches("/dev/");
                    if let Ok(info_stdout) = self
                        .run("diskutil", &args!["info", "-plist", identifier])
                        .await
                    {
                        let _ = disk.apply_diskutil_info(&info_stdout);
                    }
                }

                Ok(disks)
            }
            MachineKind::Linux => {
                let stdout = self.run(
                    "lsblk",
                    &args![
                        "-P",
                        "-o",
                        "NAME,PATH,SIZE,TYPE,MOUNTPOINTS,MODEL,PKNAME,FSTYPE,LABEL,RM,HOTPLUG,TRAN"
                    ],
                ).await?;
                Ok(stdout.parse_lsblk())
            }
            MachineKind::Unknown => bail!("System does not yet support the disks feature"),
        }
    }
}

impl Default for Machine {
    fn default() -> Self {
        Machine {
            uuid: Uuid::new_v4(),
            id: "localhost".to_string(),
            system_info: SystemInfo {
                machine_name: "localhost".into(),
                os_version: "0.1.1".into(),
                arch: "x69_42".into(),
            },
            kind: MachineKind::Unknown,
            remote: None,
            docker_path: None,
            services: MachineServices::default(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemInfo {
    pub machine_name: String,
    pub os_version: String,
    pub arch: String,
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

    pub fn get_kind(machine: &Machine) -> Self {
        Self::get_kind_from_info(&machine.system_info)
    }

    pub fn get_kind_from_info(info: &SystemInfo) -> Self {
        match &info.os_version {
            s if s.contains("Darwin") => Self::MacOS,
            s if s.contains("Linux") => Self::Linux,
            _ => Self::default(),
        }
    }
}
