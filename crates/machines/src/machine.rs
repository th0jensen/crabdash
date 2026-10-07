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
    /// Snapshot identity and command transport without copying cached inventories.
    ///
    /// Authentication and shared SSH session ownership are retained so background
    /// commands can reconnect with the same credentials as the source machine.
    pub fn command_snapshot(&self) -> Self {
        Self {
            uuid: self.uuid,
            id: self.id.clone(),
            system_info: self.system_info.clone(),
            kind: self.kind,
            remote: self.remote.clone(),
            docker_path: self.docker_path.clone(),
            services: MachineServices::default(),
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use utils::{container::Container, disks::Disk, services::ServiceItem};

    #[test]
    fn command_snapshot_preserves_authentication_and_shared_transport() -> Result<()> {
        let authentication = [
            Some(AuthMethod::Password("test-password".into())),
            Some(AuthMethod::AuthKey {
                pubkey: Some(PathBuf::from("test-key.pub")),
                privatekey: PathBuf::from("test-key"),
                passphrase: Some("test-passphrase".into()),
            }),
            Some(AuthMethod::None),
            None,
        ];
        for auth in authentication {
            let mut remote = RemoteConnection::default();
            remote.user = "operator".into();
            remote.host = "example.invalid".into();
            remote.auth = auth;
            remote.set_connected(true);
            let machine = Machine {
                kind: MachineKind::Linux,
                remote: Some(remote),
                ..Default::default()
            };
            let snapshot = machine.command_snapshot();
            let source = machine
                .remote
                .as_ref()
                .ok_or_else(|| anyhow!("Source transport"))?;
            let target = snapshot
                .remote
                .as_ref()
                .ok_or_else(|| anyhow!("Snapshot transport"))?;
            assert_eq!(target.user, source.user);
            assert_eq!(target.host, source.host);
            assert!(target.shares_session_with(source));
            assert!(snapshot.connected());
            match (source.auth.as_ref(), target.auth.as_ref()) {
                (Some(AuthMethod::Password(expected)), Some(AuthMethod::Password(actual))) => {
                    assert_eq!(actual, expected);
                }
                (
                    Some(AuthMethod::AuthKey {
                        pubkey,
                        privatekey,
                        passphrase,
                    }),
                    Some(AuthMethod::AuthKey {
                        pubkey: actual_pubkey,
                        privatekey: actual_privatekey,
                        passphrase: actual_passphrase,
                    }),
                ) => {
                    assert_eq!(actual_pubkey, pubkey);
                    assert_eq!(actual_privatekey, privatekey);
                    assert_eq!(actual_passphrase, passphrase);
                }
                (Some(AuthMethod::None), Some(AuthMethod::None)) | (None, None) => {}
                _ => anyhow::bail!("Snapshot changed authentication"),
            }
            target.set_connected(false);
            assert!(!machine.connected());
            source.set_connected(true);
            assert!(snapshot.connected());
        }
        Ok(())
    }

    #[test]
    fn command_snapshot_preserves_identity_and_leaves_source_caches_untouched() {
        let machine = Machine {
            id: "localhost-with-caches".into(),
            kind: MachineKind::Windows,
            system_info: SystemInfo {
                machine_name: "workstation".into(),
                os_version: "Windows".into(),
                arch: "x86_64".into(),
                distribution: None,
            },
            docker_path: Some("C:\\tools\\docker.exe".into()),
            services: MachineServices {
                docker: vec![Container {
                    id: "container-id".into(),
                    ..Default::default()
                }],
                disks: vec![Disk {
                    id: "disk-id".into(),
                    ..Default::default()
                }],
                systemd: vec![ServiceItem {
                    id: "17".into(),
                    name: "service-name".into(),
                    status: "active".into(),
                    description: Some("service description".into()),
                    load_state: Some("loaded".into()),
                    sub_state: Some("running".into()),
                    unit_file_state: Some("enabled".into()),
                    error: None,
                }],
                docker_error: Some("docker cached error".into()),
                docker_not_installed: true,
                disks_error: Some("disks cached error".into()),
                systemd_error: Some("services cached error".into()),
            },
            ..Default::default()
        };
        let snapshot = machine.command_snapshot();
        assert_eq!(snapshot.uuid, machine.uuid);
        assert_eq!(snapshot.id, machine.id);
        assert!(matches!(snapshot.kind, MachineKind::Windows));
        assert_eq!(snapshot.system_info, machine.system_info);
        assert_eq!(snapshot.docker_path, machine.docker_path);
        assert!(snapshot.remote.is_none());
        assert!(snapshot.connected());
        assert!(snapshot.services.docker.is_empty());
        assert!(snapshot.services.disks.is_empty());
        assert!(snapshot.services.systemd.is_empty());
        assert!(snapshot.services.docker_error.is_none());
        assert!(!snapshot.services.docker_not_installed);
        assert!(snapshot.services.disks_error.is_none());
        assert!(snapshot.services.systemd_error.is_none());
        assert_eq!(machine.services.docker.len(), 1);
        assert_eq!(machine.services.docker[0].id, "container-id");
        assert_eq!(machine.services.disks.len(), 1);
        assert_eq!(machine.services.disks[0].id, "disk-id");
        assert_eq!(machine.services.systemd.len(), 1);
        assert_eq!(machine.services.systemd[0].name, "service-name");
        assert_eq!(
            machine.services.docker_error.as_deref(),
            Some("docker cached error")
        );
        assert!(machine.services.docker_not_installed);
        assert_eq!(
            machine.services.disks_error.as_deref(),
            Some("disks cached error")
        );
        assert_eq!(
            machine.services.systemd_error.as_deref(),
            Some("services cached error")
        );
    }
}
