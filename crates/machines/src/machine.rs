use crate::remote_connection::{AuthMethod, RemoteConnection};
pub use crate::system_info::{LinuxDistribution, MachineKind, SystemInfo};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use services::MachineServices;
use smol::process::Command;
use utils::{args::Args, output::Output};
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
                let mut command = std::process::Command::new(cmd);
                command.args(args);
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt as _;
                    // Discovery/actions are background operations. Interactive
                    // terminals use ConPTY separately and keep their console.
                    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
                }
                let result = Command::from(command).output().await?;
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
                distribution: None,
            },
            kind: MachineKind::Unknown,
            remote: None,
            docker_path: None,
            services: MachineServices::default(),
        }
    }
}
