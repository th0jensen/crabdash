//! Identity probes account for Windows SSH hosts that also provide Unix tools.
use super::{MachineKind, SystemInfo, windows};
use crate::machine::Machine;
use anyhow::{Context as _, Result};
use utils::{args, args::Args};

trait Probes {
    async fn unix(&mut self) -> Result<SystemInfo>;
    async fn windows(&mut self) -> Result<SystemInfo>;
}

impl Probes for Machine {
    async fn unix(&mut self) -> Result<SystemInfo> {
        Ok(SystemInfo {
            machine_name: self.run("uname", &args!["-n"]).await?.into(),
            os_version: self.run("uname", &args!["-sr"]).await?.into(),
            arch: self.run("uname", &args!["-m"]).await?.into(),
            distribution: None,
        })
    }

    async fn windows(&mut self) -> Result<SystemInfo> {
        windows::identity(self).await
    }
}

pub(super) async fn query(machine: &mut Machine) -> Result<SystemInfo> {
    let remote = machine.remote.is_some();
    resolve(machine, remote, cfg!(target_os = "windows")).await
}

async fn resolve(
    probes: &mut impl Probes,
    remote: bool,
    native_windows: bool,
) -> Result<SystemInfo> {
    if native_windows && !remote {
        return probes.windows().await;
    }
    match probes.unix().await {
        Ok(info) => {
            if remote && matches!(MachineKind::get_kind_from_info(&info), MachineKind::Unknown) {
                // Cygwin/MSYS uname can succeed on native Windows. Recognized
                // Linux (including WSL) and Darwin never take this fallback.
                match probes.windows().await {
                    Ok(windows) => return Ok(windows),
                    Err(error) => {
                        tracing::debug!(%error, uname = %info.os_version,
                            "Unknown Unix identity; Windows identity probe failed");
                    }
                }
            }
            Ok(info)
        }
        Err(error) if remote => probes.windows().await.context(format!(
            "Unix identity unavailable ({error}); Windows identity probe failed"
        )),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{Probes, resolve};
    use crate::system_info::SystemInfo;
    use anyhow::{Result, anyhow};

    struct Fake {
        unix: Option<Result<SystemInfo>>,
        windows: Option<Result<SystemInfo>>,
        calls: Vec<&'static str>,
    }

    impl Probes for Fake {
        async fn unix(&mut self) -> Result<SystemInfo> {
            self.calls.push("unix");
            self.unix
                .take()
                .ok_or_else(|| anyhow!("unexpected Unix probe"))?
        }
        async fn windows(&mut self) -> Result<SystemInfo> {
            self.calls.push("windows");
            self.windows
                .take()
                .ok_or_else(|| anyhow!("unexpected Windows probe"))?
        }
    }

    fn info(version: &str) -> SystemInfo {
        SystemInfo {
            machine_name: "target".into(),
            os_version: version.into(),
            arch: "x86_64".into(),
            distribution: None,
        }
    }

    #[tokio::test]
    async fn successful_windows_uname_variants_use_native_identity() -> Result<()> {
        for version in [
            "CYGWIN_NT-10.0 3.6.4",
            "MINGW64_NT-10.0 3.6.4",
            "MSYS_NT-10.0 3.6.4",
        ] {
            let native = info("Windows 10.0.26100 (Microsoft Windows 11 Pro)");
            let mut probes = Fake {
                unix: Some(Ok(info(version))),
                windows: Some(Ok(native.clone())),
                calls: Vec::new(),
            };
            assert_eq!(resolve(&mut probes, true, false).await?, native);
            assert_eq!(probes.calls, ["unix", "windows"]);
        }
        Ok(())
    }

    #[tokio::test]
    async fn failed_windows_probe_preserves_successful_unknown_uname() -> Result<()> {
        let original = info("FreeBSD 14.3-RELEASE");
        let mut probes = Fake {
            unix: Some(Ok(original.clone())),
            windows: Some(Err(anyhow!("PowerShell unavailable"))),
            calls: Vec::new(),
        };
        assert_eq!(resolve(&mut probes, true, false).await?, original);
        assert_eq!(probes.calls, ["unix", "windows"]);
        Ok(())
    }

    #[tokio::test]
    async fn recognized_ssh_targets_never_probe_windows_even_from_a_windows_host() -> Result<()> {
        for version in [
            "Linux 6.12",
            "Linux 6.6.87.2-microsoft-standard-WSL2",
            "Darwin 25.0.0",
        ] {
            let original = info(version);
            let mut probes = Fake {
                unix: Some(Ok(original.clone())),
                windows: None,
                calls: Vec::new(),
            };
            assert_eq!(resolve(&mut probes, true, true).await?, original);
            assert_eq!(probes.calls, ["unix"]);
        }
        Ok(())
    }

    #[tokio::test]
    async fn failed_remote_uname_uses_windows_and_keeps_both_probe_errors() -> Result<()> {
        let native = info("Windows 11");
        let mut probes = Fake {
            unix: Some(Err(anyhow!("uname not found"))),
            windows: Some(Ok(native.clone())),
            calls: Vec::new(),
        };
        assert_eq!(resolve(&mut probes, true, false).await?, native);
        assert_eq!(probes.calls, ["unix", "windows"]);
        probes.unix = Some(Err(anyhow!("uname not found")));
        probes.windows = Some(Err(anyhow!("CIM query failed")));
        let error = resolve(&mut probes, true, false)
            .await
            .err()
            .ok_or_else(|| anyhow!("expected both identity probes to fail"))?;
        let message = format!("{error:#}");
        assert!(message.contains("uname not found"));
        assert!(message.contains("CIM query failed"));
        Ok(())
    }

    #[tokio::test]
    async fn local_native_windows_bypasses_uname_and_other_local_targets_do_not_fallback()
    -> Result<()> {
        let native = info("Windows 11");
        let mut probes = Fake {
            unix: None,
            windows: Some(Ok(native.clone())),
            calls: Vec::new(),
        };
        assert_eq!(resolve(&mut probes, false, true).await?, native);
        assert_eq!(probes.calls, ["windows"]);
        let original = info("UnknownUnix 1");
        probes.calls.clear();
        probes.unix = Some(Ok(original.clone()));
        assert_eq!(resolve(&mut probes, false, false).await?, original);
        assert_eq!(probes.calls, ["unix"]);
        probes.calls.clear();
        probes.unix = Some(Err(anyhow!("local uname failed")));
        assert!(resolve(&mut probes, false, false).await.is_err());
        assert_eq!(probes.calls, ["unix"]);
        Ok(())
    }
}
