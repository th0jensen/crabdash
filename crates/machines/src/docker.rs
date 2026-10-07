//! Docker CLI operations shared by local and SSH machines.
use crate::{
    machine::{Machine, MachineKind},
    powershell,
    store::MachineStore,
};
use anyhow::Result;
use services::docker::{Docker, DockerAction, DockerNotInstalled};
use utils::{args, args::Args, container::Container, output::Output};

const DOCKER_PATHS: &[&str] = &[
    "/opt/homebrew/bin/docker",
    "/usr/local/bin/docker",
    "/usr/bin/docker",
];

fn executable(path: &std::path::Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(target_os = "windows")]
    {
        windows_executable_name(path)
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_executable_name(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

fn resolve_local_docker(cached: Option<&str>) -> Option<String> {
    #[cfg(target_os = "windows")]
    let named = cached.into_iter();
    #[cfg(not(target_os = "windows"))]
    let named = cached.into_iter().chain(DOCKER_PATHS.iter().copied());
    let name = if cfg!(target_os = "windows") {
        "docker.exe"
    } else {
        "docker"
    };
    let search = std::env::var_os("PATH").into_iter().flat_map(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join(name))
            .collect::<Vec<_>>()
    });
    #[cfg(target_os = "windows")]
    let defaults = [
        ("ProgramFiles", "Docker/Docker/resources/bin/docker.exe"),
        (
            "LOCALAPPDATA",
            "Programs/DockerDesktop/resources/bin/docker.exe",
        ),
    ]
    .into_iter()
    .filter_map(|(variable, path)| {
        std::env::var_os(variable).map(|directory| std::path::PathBuf::from(directory).join(path))
    });
    #[cfg(not(target_os = "windows"))]
    let defaults = dirs::home_dir()
        .into_iter()
        .map(|home| home.join(".docker/bin/docker"));
    resolve_executable(
        named
            .map(std::path::PathBuf::from)
            .chain(search)
            .chain(defaults),
    )
}

fn resolve_executable(paths: impl IntoIterator<Item = std::path::PathBuf>) -> Option<String> {
    paths
        .into_iter()
        .find(|path| executable(path))
        .map(|path| path.to_string_lossy().into_owned())
}

fn windows_discovery_script(cached: Option<&str>) -> Result<String> {
    let cached = powershell::literal(cached.unwrap_or_default())?;
    Ok(format!(
        r#"
        $candidates = @({cached})
        if ($env:ProgramFiles) {{ $candidates += Join-Path $env:ProgramFiles 'Docker\Docker\resources\bin\docker.exe' }}
        if ($env:LOCALAPPDATA) {{ $candidates += Join-Path $env:LOCALAPPDATA 'Programs\DockerDesktop\resources\bin\docker.exe' }}
        foreach ($candidate in $candidates) {{
            if ($candidate -and [System.IO.Path]::GetExtension($candidate) -ieq '.exe' -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {{
                [Console]::Out.Write([System.IO.Path]::GetFullPath($candidate))
                exit 0
            }}
        }}
        $command = Get-Command -Name 'docker.exe' -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($command) {{ [Console]::Out.Write($command.Source) }}
        exit 0
    "#
    ))
}

fn posix_command(docker: &str, args: &Args) -> Args {
    let mut command = args!["-c", "exec \"$0\" \"$@\"", docker];
    command.0.extend(args.iter().cloned());
    command
}

async fn run_docker(machine: &mut Machine, docker: &str, args: &Args) -> Result<Output> {
    if machine.remote.is_some() && matches!(machine.kind, MachineKind::Windows) {
        powershell::run_native(machine, docker, args).await
    } else if machine.remote.is_some() {
        machine.run("sh", &posix_command(docker, args)).await
    } else {
        machine.run(docker, args).await
    }
}

#[cfg(test)]
mod windows_discovery_tests {
    use super::*;

    #[test]
    fn windows_cached_cli_names_require_an_executable_extension() {
        for path in ["docker.exe", "DOCKER.EXE", "Docker.ExE"] {
            assert!(windows_executable_name(std::path::Path::new(path)));
        }
        for path in [
            "docker",
            "docker.cmd",
            "docker.ps1",
            "docker.exe.txt",
            "/usr/bin/docker",
        ] {
            assert!(!windows_executable_name(std::path::Path::new(path)));
        }
    }

    #[test]
    fn cached_windows_paths_are_data_and_missing_cli_is_successful_discovery() -> Result<()> {
        let cached = "C:\\user's ‘folder’\\docker.exe";
        let script = windows_discovery_script(Some(cached))?;
        assert!(script.contains(&format!(
            "$candidates = @({})",
            powershell::literal(cached)?
        )));
        assert!(script.contains("Test-Path -LiteralPath $candidate -PathType Leaf"));
        assert!(script.contains("-CommandType Application"));
        assert!(script.trim_end().ends_with("exit 0"));
        assert!(windows_discovery_script(None)?.contains("$candidates = @('')"));
        assert!(windows_discovery_script(Some("bad\0path")).is_err());
        Ok(())
    }
}

impl Docker for Machine {
    async fn find_docker(&mut self) -> Result<String> {
        let path = if self.remote.is_some() && matches!(self.kind, MachineKind::Windows) {
            let script = windows_discovery_script(self.docker_path.as_deref())?;
            let path = String::from(powershell::run(self, &script).await?);
            (!path.is_empty()).then_some(path)
        } else if self.remote.is_some() {
            // Probe without failing the shell: missing Docker is ordinary state,
            // while an SSH failure remains a transport error.
            let mut args = args![
                "-c",
                r#"for path in "$@" "$HOME/.docker/bin/docker"; do if [ -f "$path" ] && [ -x "$path" ]; then printf '%s' "$path"; exit; fi; done; command -v docker || true"#,
                "crabdash-docker"
            ];
            if let Some(cached) = &self.docker_path {
                args.push(cached.clone());
            }
            args.0
                .extend(DOCKER_PATHS.iter().map(|path| (*path).to_owned()));
            let output = self.run("sh", &args).await?;
            let path = String::from(output).trim().to_owned();
            (!path.is_empty()).then_some(path)
        } else {
            resolve_local_docker(self.docker_path.as_deref())
        };
        let path = path.ok_or(DockerNotInstalled)?;
        if self.docker_path.as_ref() != Some(&path) {
            self.docker_path = Some(path.clone());
            if let Err(error) = MachineStore::update_machine(self.clone()).await {
                tracing::debug!(%error, "Could not cache the Docker executable path");
            }
        }
        Ok(path)
    }

    async fn list_docker(&mut self) -> Result<Vec<Container>> {
        let args = args!["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.State}}"];
        let docker = self.find_docker().await?;
        Ok(utils::container::parse(
            &run_docker(self, &docker, &args).await?,
        ))
    }

    async fn container_action(&mut self, id: &str, action: DockerAction) -> Result<Output> {
        if let DockerAction::Remove { force } = action {
            return self.remove_container(id, force).await;
        }
        let args = args![action.command(), "--", id];
        let docker = self.find_docker().await?;
        run_docker(self, &docker, &args).await
    }

    async fn run_container(&mut self, args: &Args) -> Result<Output> {
        let docker = self.find_docker().await?;
        let mut command = args!["run"];
        command.0.extend(args.iter().cloned());
        run_docker(self, &docker, &command).await
    }

    async fn remove_container(&mut self, id: &str, force: bool) -> Result<Output> {
        let docker = self.find_docker().await?;
        if !force {
            // Recheck current state: the confirmation may have been open while
            // Docker's state changed. Stop gracefully and keep all volumes.
            if let Some(container) = self.list_docker().await?.iter().find(|c| c.id == id) {
                if container.is_paused() {
                    run_docker(self, &docker, &args!["unpause", "--", id]).await?;
                }
                if container.is_active_status() {
                    run_docker(self, &docker, &args!["stop", "--", id]).await?;
                }
            }
        }
        let args = if force {
            args!["rm", "--force", "--", id]
        } else {
            args!["rm", "--", id]
        };
        run_docker(self, &docker, &args).await
    }

    async fn container_logs(&mut self, id: &str, lines: u32) -> Result<Output> {
        let docker = self.find_docker().await?;
        run_docker(
            self,
            &docker,
            &args!["logs", "--tail", &lines.to_string(), "--", id],
        )
        .await
    }
}

#[cfg(all(test, unix))]
mod docker_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use uuid::Uuid;

    struct Fixture {
        directory: std::path::PathBuf,
        machine: Machine,
    }

    impl Fixture {
        fn new(state: &str, fail_stop: bool) -> Result<Self> {
            let directory =
                std::env::temp_dir().join(format!("crabdash-docker-test-{}", Uuid::new_v4()));
            std::fs::create_dir(&directory)?;
            let script = directory.join("docker");
            std::fs::write(
                &script,
                format!(
                    r#"#!/bin/sh
printf '%s\n' "$*" >> '{}/calls'
case "$1" in
  ps) printf 'abc123\tfixture\t{}\n' ;;
  stop) {} ;;
esac
"#,
                    directory.display(),
                    state,
                    if fail_stop {
                        "echo 'stop failed' >&2; exit 1"
                    } else {
                        ":"
                    }
                ),
            )?;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
            Ok(Self {
                machine: Machine {
                    docker_path: Some(script.to_string_lossy().into()),
                    ..Default::default()
                },
                directory,
            })
        }
        fn calls(&self) -> Result<String> {
            Ok(std::fs::read_to_string(self.directory.join("calls"))?)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn remote_unix_executable_paths_are_literal_arguments() -> Result<()> {
        let fixture = Fixture::new("exited", false)?;
        let program = fixture.directory.join("docker user's $(command) `name`");
        std::fs::rename(fixture.directory.join("docker"), &program)?;
        let command = posix_command(
            &program.to_string_lossy(),
            &args!["run", "--name", "shell's $(literal)", "image"],
        );
        let output = smol::block_on(Machine::default().run("sh", &command))?;
        assert_eq!(String::from(output), "");
        assert_eq!(fixture.calls()?, "run --name shell's $(literal) image\n");
        Ok(())
    }

    #[test]
    fn discovery_skips_missing_and_non_executable_candidates() -> Result<()> {
        let fixture = Fixture::new("exited", false)?;
        let script = fixture.directory.join("docker");
        let missing = fixture.directory.join("missing-docker");
        assert_eq!(
            resolve_executable([missing.clone(), script.clone()]),
            Some(script.to_string_lossy().into_owned())
        );
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o600))?;
        assert!(resolve_executable([missing, script]).is_none());
        Ok(())
    }

    #[test]
    fn installed_docker_failures_are_not_installation_state() -> Result<()> {
        let mut fixture = Fixture::new("exited", false)?;
        std::fs::write(
            fixture.directory.join("docker"),
            "#!/bin/sh\necho 'Cannot connect to the Docker daemon' >&2\nexit 1\n",
        )?;
        let error = smol::block_on(fixture.machine.list_docker())
            .err()
            .ok_or_else(|| anyhow::anyhow!("Expected the daemon failure"))?;
        assert!(!error.is::<DockerNotInstalled>());
        assert!(
            error
                .to_string()
                .contains("Cannot connect to the Docker daemon")
        );
        Ok(())
    }

    #[test]
    fn run_uses_the_subcommand_and_preserves_each_parameter() -> Result<()> {
        let mut fixture = Fixture::new("exited", false)?;
        let script = fixture.directory.join("docker");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}/calls'\n",
                fixture.directory.display()
            ),
        )?;
        smol::block_on(fixture.machine.run_container(&args![
            "-d",
            "-e",
            "MESSAGE=hello world",
            "alpine",
            "sh",
            "-c",
            "echo hello"
        ]))?;
        assert_eq!(
            fixture.calls()?,
            "run\n-d\n-e\nMESSAGE=hello world\nalpine\nsh\n-c\necho hello\n"
        );
        Ok(())
    }

    #[test]
    fn removal_unpauses_then_stops_and_preserves_volumes() -> Result<()> {
        let mut fixture = Fixture::new("paused", false)?;
        smol::block_on(fixture.machine.remove_container("abc123", false))?;
        let calls = fixture.calls()?;
        assert!(calls.ends_with("unpause -- abc123\nstop -- abc123\nrm -- abc123\n"));
        assert!(!calls.contains("--force"));
        assert!(!calls.contains("--volumes"));
        Ok(())
    }

    #[test]
    fn removal_aborts_if_graceful_stop_fails() -> Result<()> {
        let mut fixture = Fixture::new("running", true)?;
        let error = smol::block_on(fixture.machine.remove_container("abc123", false))
            .err()
            .ok_or_else(|| anyhow::anyhow!("Expected graceful stop to fail"))?;
        assert!(error.to_string().contains("stop failed"));
        assert!(!fixture.calls()?.lines().any(|line| line.starts_with("rm ")));
        Ok(())
    }

    #[test]
    fn force_removal_requires_explicit_option() -> Result<()> {
        let mut fixture = Fixture::new("running", false)?;
        smol::block_on(fixture.machine.remove_container("abc123", true))?;
        assert_eq!(fixture.calls()?, "rm --force -- abc123\n");
        Ok(())
    }

    #[test]
    fn stopped_removal_does_not_start_or_stop_the_container() -> Result<()> {
        let mut fixture = Fixture::new("exited", false)?;
        smol::block_on(fixture.machine.remove_container("abc123", false))?;
        let calls = fixture.calls()?;
        assert_eq!(calls.lines().count(), 2);
        assert!(calls.ends_with("rm -- abc123\n"));
        Ok(())
    }
}
