//! Windows identity from CIM, independent of the desktop host platform.
use super::SystemInfo;
use crate::{machine::Machine, powershell};
use anyhow::{Context as _, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct Identity {
    name: String,
    version: String,
    arch: String,
}

pub(super) async fn identity(machine: &mut Machine) -> Result<SystemInfo> {
    let output = powershell::run(
        machine,
        r#"
        $os = Get-CimInstance -ClassName Win32_OperatingSystem
        [pscustomobject]@{
            name = [string]$env:COMPUTERNAME
            version = 'Windows ' + $os.Version + ' (' + $os.Caption + ')'
            arch = [string]$env:PROCESSOR_ARCHITECTURE
        } | ConvertTo-Json -Compress
    "#,
    )
    .await?;
    parse(&output)
}

fn parse(output: &str) -> Result<SystemInfo> {
    let identity: Identity = serde_json::from_str(output.trim_start_matches('\u{feff}'))
        .context("Invalid Windows identity response")?;
    Ok(SystemInfo {
        machine_name: identity.name,
        os_version: identity.version,
        arch: match identity.arch.as_str() {
            "AMD64" => "x86_64".into(),
            "ARM64" => "aarch64".into(),
            "x86" => "i686".into(),
            _ => identity.arch,
        },
        distribution: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::MachineKind;

    #[test]
    fn detects_windows_without_misidentifying_wsl() -> Result<()> {
        let identity = parse(
            r#"{"name":"desktop","version":"Windows 10.0.26100 (Microsoft Windows 11 Pro)","arch":"AMD64"}"#,
        )?;
        assert!(matches!(
            MachineKind::get_kind_from_info(&identity),
            MachineKind::Windows
        ));
        assert_eq!(identity.arch, "x86_64");
        assert!(matches!(
            MachineKind::get_kind_from_info(&SystemInfo {
                os_version: "Linux 6.6.87.2-microsoft-standard-WSL2".into(),
                ..SystemInfo::default()
            }),
            MachineKind::Linux
        ));
        Ok(())
    }
}
