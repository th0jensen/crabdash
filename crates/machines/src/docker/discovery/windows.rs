//! One Windows discovery path for native desktops and Windows SSH targets.
use crate::{machine::Machine, powershell};
use anyhow::Result;

const SOURCE: &str = include_str!("windows.ps1");

fn script(cached: Option<&str>) -> Result<String> {
    Ok(format!(
        r#"{SOURCE}
$paths = @($env:PATH)
foreach ($scope in @('User', 'Machine')) {{
    try {{ $paths += [Environment]::GetEnvironmentVariable('Path', $scope) }} catch {{ }}
}}
$defaults = @(
    '%ProgramW6432%\Docker\Docker\resources\bin\docker.exe',
    '%ProgramFiles%\Docker\Docker\resources\bin\docker.exe',
    '%LOCALAPPDATA%\Programs\DockerDesktop\resources\bin\docker.exe'
)
$path = Resolve-CrabdashDocker -Cached {cached} -Paths $paths -Defaults $defaults
if ($path) {{ [Console]::Out.Write($path) }}
exit 0
"#,
        cached = powershell::literal(cached.unwrap_or_default())?
    ))
}

pub(super) async fn find(machine: &mut Machine) -> Result<Option<String>> {
    let script = script(machine.docker_path.as_deref())?;
    let path = String::from(powershell::run(machine, &script).await?);
    Ok((!path.is_empty()).then_some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_paths_are_literal_and_discovery_fits_windows_ssh() -> Result<()> {
        let cached = r"%ProgramFiles%\user's ‘folder’\docker.exe";
        let source = script(Some(cached))?;
        assert!(source.contains(&format!("-Cached {}", powershell::literal(cached)?)));
        assert!(powershell::encoded(&source).len() + 80 < 8191);
        assert!(script(Some("bad\0path")).is_err());
        Ok(())
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_discovery_handles_stale_environment_and_literal_paths() -> Result<()> {
        let executable = powershell::local_executable()
            .ok_or_else(|| anyhow::anyhow!("Windows PowerShell unavailable"))?;
        let source = format!("{SOURCE}\n{}", include_str!("tests.ps1"));
        let output = std::process::Command::new(executable)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &powershell::encoded(&source),
            ])
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
}
