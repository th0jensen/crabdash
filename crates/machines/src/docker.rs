//! Docker CLI operations shared by local and SSH machines.
use crate::{
    machine::{Machine, MachineKind},
    powershell,
    store::MachineStore,
};
use anyhow::{Result, ensure};
use services::docker::{Docker, DockerAction, DockerNotInstalled};
use utils::{
    args,
    args::Args,
    container::{Container, details::ContainerDetails},
    output::Output,
};

mod discovery;

#[cfg(all(test, unix))]
use discovery::unix::resolve_executable;

fn posix_command(docker: &str, args: &Args) -> Args {
    let mut command = args!["-c", "exec \"$0\" \"$@\"", docker];
    command.0.extend(args.iter().cloned());
    command
}

fn inspect_command(id: &str) -> Args {
    args!["inspect", "--type", "container", "--", id]
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

impl Docker for Machine {
    async fn find_docker(&mut self) -> Result<String> {
        let path = discovery::find(self).await?;
        let path = path.ok_or(DockerNotInstalled)?;
        if self.docker_path.as_ref() != Some(&path) {
            self.docker_path = Some(path.clone());
            if let Err(error) = MachineStore::cache_docker_path(self.uuid, path.clone()).await {
                tracing::debug!(%error, "Could not cache the Docker executable path");
            }
        }
        Ok(path)
    }

    async fn list_docker(&mut self) -> Result<Vec<Container>> {
        let args = args![
            "ps",
            "-a",
            "--no-trunc",
            "--format",
            "{{.ID}}\t{{.Names}}\t{{.State}}"
        ];
        let docker = self.find_docker().await?;
        Ok(utils::container::parse(
            &run_docker(self, &docker, &args).await?,
        ))
    }

    async fn inspect_container(&mut self, id: &str) -> Result<ContainerDetails> {
        let docker = self.find_docker().await?;
        let output = run_docker(self, &docker, &inspect_command(id)).await?;
        let details = utils::container::details::parse(&output)?;
        ensure!(
            details.id == id,
            "Docker inspection returned a different container ID"
        );
        Ok(details)
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

    const INSPECT_ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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

    fn inspection_fixture(response: &str) -> Result<Fixture> {
        let fixture = Fixture::new("exited", false)?;
        std::fs::write(fixture.directory.join("response"), response)?;
        std::fs::write(
            fixture.directory.join("docker"),
            format!(
                "#!/bin/sh\nprintf '%s\\0' \"$@\" > '{}/inspect-args'\ncat '{}/response'\n",
                fixture.directory.display(),
                fixture.directory.display()
            ),
        )?;
        Ok(fixture)
    }

    fn inspection_response() -> String {
        format!(r#"[{{"Id":"{INSPECT_ID}","Name":"/fixture","State":{{"Status":"exited"}}}}]"#)
    }

    fn assert_inspection_arguments(fixture: &Fixture, id: &str) -> Result<()> {
        let expected = ["inspect", "--type", "container", "--", id]
            .into_iter()
            .flat_map(|arg| arg.as_bytes().iter().copied().chain(std::iter::once(0)))
            .collect::<Vec<_>>();
        assert_eq!(
            std::fs::read(fixture.directory.join("inspect-args"))?,
            expected
        );
        Ok(())
    }

    #[test]
    fn inspection_is_read_only_and_uses_the_exact_container_identifier() -> Result<()> {
        let mut fixture = inspection_fixture(&inspection_response())?;
        let details = smol::block_on(fixture.machine.inspect_container(INSPECT_ID))?;
        assert_eq!(details.id, INSPECT_ID);
        assert_eq!(details.name, "fixture");
        assert_inspection_arguments(&fixture, INSPECT_ID)?;
        assert!(!fixture.directory.join("calls").exists());
        Ok(())
    }

    #[test]
    fn inventory_requests_full_container_identifiers() -> Result<()> {
        let mut fixture = Fixture::new("exited", false)?;
        smol::block_on(fixture.machine.list_docker())?;
        assert_eq!(
            fixture.calls()?,
            "ps -a --no-trunc --format {{.ID}}\t{{.Names}}\t{{.State}}\n"
        );
        Ok(())
    }

    #[test]
    fn inspection_preserves_identifiers_as_one_literal_argument_on_local_and_posix_transports()
    -> Result<()> {
        let mut fixture = inspection_fixture(&inspection_response())?;
        let marker = fixture.directory.join("injected");
        let id = format!(
            "--format=json; touch {}; $(touch {}) user's `literal`\nnext",
            marker.display(),
            marker.display()
        );
        // The fixture returns a valid but different ID, which must be rejected.
        assert!(smol::block_on(fixture.machine.inspect_container(&id)).is_err());
        assert_inspection_arguments(&fixture, &id)?;
        assert!(!marker.exists());

        let docker = fixture.directory.join("docker");
        let command = posix_command(&docker.to_string_lossy(), &inspect_command(&id));
        smol::block_on(Machine::default().run("sh", &command))?;
        assert_inspection_arguments(&fixture, &id)?;
        assert!(!marker.exists());
        Ok(())
    }

    #[test]
    fn inspection_rejects_a_mismatched_response_and_propagates_cli_errors() -> Result<()> {
        let mut fixture = inspection_fixture(&inspection_response())?;
        let error = smol::block_on(fixture.machine.inspect_container("different-id"))
            .err()
            .ok_or_else(|| anyhow::anyhow!("Expected identifier mismatch"))?;
        assert!(error.to_string().contains("different container ID"));
        std::fs::write(
            fixture.directory.join("docker"),
            "#!/bin/sh\necho 'No such container' >&2\nexit 1\n",
        )?;
        let error = smol::block_on(fixture.machine.inspect_container(INSPECT_ID))
            .err()
            .ok_or_else(|| anyhow::anyhow!("Expected Docker error"))?;
        assert!(
            error.to_string().contains("No such container"),
            "Unexpected Docker error: {error:#}"
        );
        assert!(!error.is::<DockerNotInstalled>());
        Ok(())
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
