//! Windows identity from CIM, independent of the desktop host platform.
use super::SystemInfo;
use crate::{machine::Machine, powershell};
use anyhow::{Context as _, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct Identity {
    name: String,
    version: String,
    arch: Option<Architecture>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Architecture {
    Platform(u16),
    Legacy(String),
}

fn platform_architecture(code: u16) -> String {
    // Win32_Processor.Architecture describes the target platform, unlike the
    // command process's PROCESSOR_ARCHITECTURE environment under emulation.
    match code {
        0 => "i686".into(),
        1 => "mips".into(),
        2 => "alpha".into(),
        3 => "powerpc".into(),
        5 => "arm".into(),
        6 => "ia64".into(),
        9 => "x86_64".into(),
        12 => "aarch64".into(),
        _ => format!("Unknown ({code})"),
    }
}

fn architecture(value: Option<Architecture>) -> String {
    match value {
        Some(Architecture::Platform(code)) => platform_architecture(code),
        // Accept the former response format without changing saved SystemInfo.
        Some(Architecture::Legacy(value)) => match value.as_str() {
            "AMD64" => "x86_64".into(),
            "ARM64" => "aarch64".into(),
            "x86" => "i686".into(),
            "ARM" => "arm".into(),
            "IA64" => "ia64".into(),
            "" => "Unknown".into(),
            _ => value
                .parse::<u16>()
                .map_or_else(|_| format!("Unknown ({value})"), platform_architecture),
        },
        None => "Unknown".into(),
    }
}

pub(super) async fn identity(machine: &mut Machine) -> Result<SystemInfo> {
    let output = powershell::run(
        machine,
        r#"
        $os = Get-CimInstance -ClassName Win32_OperatingSystem
        $architecture = $null
        try {
            $processor = Get-CimInstance -ClassName Win32_Processor -Property Architecture -ErrorAction Stop | Select-Object -First 1
            if ($null -ne $processor.Architecture) { $architecture = [uint16]$processor.Architecture }
        } catch { }
        [pscustomobject]@{
            name = [string]$env:COMPUTERNAME
            version = 'Windows ' + $os.Version + ' (' + $os.Caption + ')'
            arch = $architecture
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
        arch: architecture(identity.arch),
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

    #[test]
    fn cim_platform_architecture_overrides_emulated_process_metadata() -> Result<()> {
        for (code, process_arch, expected) in [(9, "x86", "x86_64"), (12, "AMD64", "aarch64")] {
            let identity = parse(
                &serde_json::json!({
                    "name": "desktop", "version": "Windows 11", "arch": code,
                    "process_arch": process_arch,
                })
                .to_string(),
            )?;
            assert_eq!(identity.arch, expected);
            assert!(matches!(
                MachineKind::get_kind_from_info(&identity),
                MachineKind::Windows
            ));
        }
        Ok(())
    }

    #[test]
    fn documented_platform_codes_and_legacy_wire_values_are_preserved() -> Result<()> {
        for (code, expected) in [
            (0, "i686"),
            (1, "mips"),
            (2, "alpha"),
            (3, "powerpc"),
            (5, "arm"),
            (6, "ia64"),
            (9, "x86_64"),
            (12, "aarch64"),
        ] {
            let identity = parse(
                &serde_json::json!({"name":"desktop", "version":"Windows 11", "arch":code})
                    .to_string(),
            )?;
            assert_eq!(identity.arch, expected);
        }
        for (value, expected) in [
            ("AMD64", "x86_64"),
            ("ARM64", "aarch64"),
            ("x86", "i686"),
            ("12", "aarch64"),
        ] {
            let identity = parse(
                &serde_json::json!({"name":"desktop", "version":"Windows 11", "arch":value})
                    .to_string(),
            )?;
            assert_eq!(identity.arch, expected);
        }
        Ok(())
    }

    #[test]
    fn unknown_missing_and_null_architecture_do_not_become_process_architecture() -> Result<()> {
        for (value, expected) in [
            (serde_json::json!(65535), "Unknown (65535)"),
            (serde_json::json!(4), "Unknown (4)"),
            (serde_json::json!("future"), "Unknown (future)"),
            (serde_json::Value::Null, "Unknown"),
        ] {
            let identity = parse(&serde_json::json!({"name":"desktop", "version":"Windows 11", "arch":value, "process_arch":"AMD64"}).to_string())?;
            assert_eq!(identity.arch, expected);
        }
        assert_eq!(
            parse(r#"{"name":"desktop","version":"Windows 11"}"#)?.arch,
            "Unknown"
        );
        for value in [
            serde_json::json!(-1),
            serde_json::json!(65536),
            serde_json::json!(9.5),
        ] {
            assert!(
                parse(
                    &serde_json::json!({"name":"desktop", "version":"Windows 11", "arch":value})
                        .to_string()
                )
                .is_err()
            );
        }
        Ok(())
    }
}
