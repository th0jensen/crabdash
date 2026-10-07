//! Identity and platform detection for a selected machine.
mod linux;
mod macos;
mod windows;
use crate::{machine::Machine, store::MachineStore};
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use utils::{args, args::Args};

impl Machine {
    /// Refreshes system information from the machine and updates internal state
    /// if it has changed.
    ///
    /// If the newly retrieved info differs from the current state, the machine
    /// kind is re-evaluated and the updated machine is persisted via
    /// [`MachineStore::update_system_info`], preserving other saved fields.
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

        MachineStore::update_system_info(self.uuid, self.system_info.clone(), self.kind).await?;
        Ok(true)
    }

    pub(crate) async fn get_system_info(&mut self) -> Result<SystemInfo> {
        if cfg!(target_os = "windows") && self.remote.is_none() {
            return windows::identity(self).await;
        }
        let cmd = "uname";
        let unix_identity = async {
            Ok::<_, anyhow::Error>((
                self.run(cmd, &args!["-n"]).await?.into(),
                self.run(cmd, &args!["-sr"]).await?.into(),
                self.run(cmd, &args!["-m"]).await?.into(),
            ))
        }
        .await;
        let (machine_name, os_version, arch) = match unix_identity {
            Ok(identity) => identity,
            Err(error) if self.remote.is_some() => {
                return windows::identity(self).await.context(format!(
                    "Unix identity unavailable ({error}); Windows identity probe failed"
                ));
            }
            Err(error) => return Err(error),
        };
        let mut info = SystemInfo {
            machine_name,
            os_version,
            arch,
            distribution: None,
        };
        let distribution = match MachineKind::get_kind_from_info(&info) {
            MachineKind::Linux => linux::distribution(self).await,
            MachineKind::MacOS => macos::distribution(self).await,
            MachineKind::Windows => Ok(None),
            MachineKind::Unknown => Ok(None),
        };
        info.distribution = distribution.unwrap_or_else(|error| {
            tracing::debug!(%error, "Could not refresh distribution metadata");
            // An unavailable os-release must not break machine refresh or erase
            // a previously detected Linux distribution during a transient error.
            if matches!(MachineKind::get_kind_from_info(&info), MachineKind::Linux) {
                self.system_info.distribution.clone()
            } else {
                None
            }
        });
        Ok(info)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemInfo {
    pub machine_name: String,
    pub os_version: String,
    pub arch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distribution: Option<LinuxDistribution>,
}

/// Vendor-provided identity from the selected Linux machine's os-release file.
/// Logo selection belongs to the app; machine metadata contains no asset paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinuxDistribution {
    pub id: String,
    pub name: String,
    pub pretty_name: String,
}

impl SystemInfo {
    pub fn platform_label(&self) -> &str {
        self.distribution.as_ref().map_or_else(
            || MachineKind::get_kind_from_info(self).label(),
            |distribution| distribution.name.as_str(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_machines_without_distribution_metadata_still_load() {
        let info: SystemInfo = serde_json::from_str(
            r#"{"machine_name":"fedora","os_version":"Linux 7.2","arch":"aarch64"}"#,
        )
        .unwrap();
        assert!(info.distribution.is_none());
        assert_eq!(info.platform_label(), "Linux");
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub enum MachineKind {
    MacOS,
    Linux,
    Windows,
    #[default]
    Unknown,
}

impl MachineKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::MacOS => "macOS",
            Self::Linux => "Linux",
            Self::Windows => "Windows",
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
            s if s.starts_with("Windows ") || s.contains("Microsoft Windows") => Self::Windows,
            _ => Self::default(),
        }
    }
}
