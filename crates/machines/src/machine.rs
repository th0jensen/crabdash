use crate::{
    commands::SystemInfo,
    remote_connection::{AuthMethod, RemoteConnection},
    store::MachineStore,
};
use anyhow::{Result, anyhow, bail};
use indoc::indoc;
use serde::{Deserialize, Serialize};
use services::{
    Disk, MachineServices, ServiceItem, Services,
    disks::Disks,
    docker::{Container, Docker},
};
use std::{iter::once, process::Command};
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
    pub fn new_remote(user: &str, host: &str, auth: AuthMethod) -> Result<Self> {
        let mut rc = RemoteConnection::new_connection(user, host, auth)?;
        let system_info = SystemInfo::remote(&mut rc)?;

        Ok(Self {
            uuid: Uuid::new_v4(),
            id: format!("{user}@{host}"),
            kind: MachineKind::get_kind(&system_info),
            system_info,
            remote: Some(rc),
            docker_path: None,
            services: MachineServices::default(),
        })
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
    /// Creates a lightweight clone of this machine with a fresh service state.
    ///
    /// All fields are cloned from the original except `services`, which is
    /// reset to [`MachineServices::default`]. Intended for use when the same
    /// machine connection needs to be shared across background tasks.
    ///
    /// Note: if the machine has an active SSH connection, the underlying
    /// connection handle is cloned and shared — not duplicated.
    pub fn background_clone(&self) -> Self {
        Self {
            uuid: self.uuid.clone(),
            id: self.id.clone(),
            system_info: self.system_info.clone(),
            kind: self.kind,
            remote: self.remote.clone(),
            docker_path: self.docker_path.clone(),
            services: MachineServices::default(),
        }
    }
    /// Returns whether the machine is reachable and has an active connection.
    ///
    /// For remote machines, delegates to the underlying SSH session state.
    /// For local machines, always returns `true`.
    pub fn has_active_connection(&self) -> bool {
        self.remote
            .as_ref()
            .map_or(true, |remote| remote.has_active_session())
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

    fn list_docker(&mut self) -> Result<Vec<Container>> {
        let args = vec!["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.State}}"];
        let docker = self.find_docker();
        let stdout = self.run(&docker, Some(&args))?;
        Ok(Container::parse_output(stdout))
    }

    fn container_action(&mut self, id: &str, action: &str) -> Result<String> {
        let args = vec![action, id];
        let docker = self.find_docker();
        let stdout = self.run(&docker, Some(&args))?;
        if stdout.trim() != id {
            bail!(stdout)
        }
        Ok(stdout)
    }

    fn run_container(&mut self, args: Vec<String>) -> Result<String> {
        let docker = self.find_docker();
        let args: Vec<&str> = once("run").chain(args.iter().map(String::as_str)).collect();
        let stdout = self.run(&docker, Some(&args))?;
        Ok(stdout)
    }

    fn container_logs(&mut self, id: &str) -> Result<String> {
        let docker = self.find_docker();
        let stdout = self.run(&docker, Some(&["logs", id]))?;
        Ok(stdout)
    }
}

impl Services for Machine {
    fn list_services(&mut self) -> Result<Vec<ServiceItem>> {
        let (command, args): (&str, Option<&[&str]>) = match self.kind {
            MachineKind::MacOS => ("launchctl", Some(&["list"])),
            MachineKind::Linux => (
                "sh",
                Some(&[
                    "-c",
                    indoc! {r#"
                        systemctl list-units --type=service --all --no-legend --no-pager \
                        | awk '{print $1}' \
                        | while read -r unit; do
                            status=$(systemctl show -p ActiveState --value "$unit")
                            pid=$(systemctl show -p MainPID --value "$unit")
                            printf "%s\t%s\t%s\n" "$pid" "$status" "$unit"
                        done
                    "#},
                ]),
            ),
            _ => bail!("System does not yet support the services feature"),
        };

        Ok(ServiceItem::parse_output(self.run(command, args)?))
    }
}

impl Disks for Machine {
    fn list_disks(&mut self) -> Result<Vec<Disk>> {
        match self.kind {
            MachineKind::MacOS => {
                let list_stdout = self.run("diskutil", Some(&["list", "-plist"]))?;
                let apfs_stdout = self.run("diskutil", Some(&["apfs", "list", "-plist"])).ok();
                let mut disks = Disk::convert_diskutil(&list_stdout, apfs_stdout.as_deref())?;

                for disk in &mut disks {
                    let identifier = disk.id.trim_start_matches("/dev/");
                    if let Ok(info_stdout) =
                        self.run("diskutil", Some(&["info", "-plist", identifier]))
                    {
                        let _ = disk.apply_diskutil_info(&info_stdout);
                    }
                }

                Ok(disks)
            }
            MachineKind::Linux => {
                let stdout = self.run(
                    "lsblk",
                    Some(&[
                        "-P",
                        "-o",
                        "NAME,PATH,SIZE,TYPE,MOUNTPOINTS,MODEL,PKNAME,FSTYPE,LABEL,RM,HOTPLUG,TRAN",
                    ]),
                )?;
                Ok(Disk::convert_lsblk(&stdout))
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
