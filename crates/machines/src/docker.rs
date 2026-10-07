//! Docker CLI operations shared by local and SSH machines.
use crate::{machine::Machine, store::MachineStore};
use anyhow::Result;
use services::docker::{Docker, DockerAction};
use utils::{args, args::Args, container::Container, output::Output};

impl Docker for Machine {
    async fn find_docker(&mut self) -> String {
        if let Some(path) = &self.docker_path {
            return path.clone();
        }

        const CANDIDATES: &[&str] = &[
            "/opt/homebrew/bin/docker",
            "/usr/local/bin/docker",
            "/usr/bin/docker",
        ];

        let path = {
            let mut found = None;
            for p in CANDIDATES.iter().copied() {
                if self.run("test", &args!["-f", p]).await.is_ok() {
                    found = Some(p.to_string());
                    break;
                }
            }
            found.unwrap_or_else(|| String::from("docker"))
        };

        self.docker_path = Some(path.clone());
        MachineStore::update_machine(self.clone()).await.ok();
        path
    }

    async fn list_docker(&mut self) -> Result<Vec<Container>> {
        let args = args!["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.State}}"];
        let docker = self.find_docker().await;
        Ok(utils::container::parse(&self.run(&docker, &args).await?))
    }

    async fn container_action(&mut self, id: &str, action: DockerAction) -> Result<Output> {
        if let DockerAction::Remove { force } = action {
            return self.remove_container(id, force).await;
        }
        let args = args![action.command(), "--", id];
        let docker = self.find_docker().await;
        Ok(self.run(&docker, &args).await?)
    }

    async fn run_container(&mut self, args: &Args) -> Result<Output> {
        let docker = self.find_docker().await;
        let mut command = args!["run"];
        command.0.extend(args.iter().cloned());
        self.run(&docker, &command).await
    }

    async fn remove_container(&mut self, id: &str, force: bool) -> Result<Output> {
        let docker = self.find_docker().await;
        if !force {
            // Recheck current state: the confirmation may have been open while
            // Docker's state changed. Stop gracefully and keep all volumes.
            if let Some(container) = self.list_docker().await?.iter().find(|c| c.id == id) {
                if container.is_paused() {
                    self.run(&docker, &args!["unpause", "--", id]).await?;
                }
                if container.is_active_status() {
                    self.run(&docker, &args!["stop", "--", id]).await?;
                }
            }
        }
        let args = if force {
            args!["rm", "--force", "--", id]
        } else {
            args!["rm", "--", id]
        };
        self.run(&docker, &args).await
    }

    async fn container_logs(&mut self, id: &str, lines: u32) -> Result<Output> {
        let docker = self.find_docker().await;
        Ok(self
            .run(&docker, &args!["logs", "--tail", &lines.to_string(), id])
            .await?)
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
        fn new(state: &str, fail_stop: bool) -> Self {
            let directory =
                std::env::temp_dir().join(format!("crabdash-docker-test-{}", Uuid::new_v4()));
            std::fs::create_dir(&directory).unwrap();
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
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                machine: Machine {
                    docker_path: Some(script.to_string_lossy().into()),
                    ..Default::default()
                },
                directory,
            }
        }
        fn calls(&self) -> String {
            std::fs::read_to_string(self.directory.join("calls")).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn run_uses_the_subcommand_and_preserves_each_parameter() {
        let mut fixture = Fixture::new("exited", false);
        let script = fixture.directory.join("docker");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}/calls'\n",
                fixture.directory.display()
            ),
        )
        .unwrap();
        smol::block_on(fixture.machine.run_container(&args![
            "-d",
            "-e",
            "MESSAGE=hello world",
            "alpine",
            "sh",
            "-c",
            "echo hello"
        ]))
        .unwrap();
        assert_eq!(
            fixture.calls(),
            "run\n-d\n-e\nMESSAGE=hello world\nalpine\nsh\n-c\necho hello\n"
        );
    }

    #[test]
    fn removal_unpauses_then_stops_and_preserves_volumes() {
        let mut fixture = Fixture::new("paused", false);
        smol::block_on(fixture.machine.remove_container("abc123", false)).unwrap();
        let calls = fixture.calls();
        assert!(calls.ends_with("unpause -- abc123\nstop -- abc123\nrm -- abc123\n"));
        assert!(!calls.contains("--force"));
        assert!(!calls.contains("--volumes"));
    }

    #[test]
    fn removal_aborts_if_graceful_stop_fails() {
        let mut fixture = Fixture::new("running", true);
        let error = smol::block_on(fixture.machine.remove_container("abc123", false)).unwrap_err();
        assert!(error.to_string().contains("stop failed"));
        assert!(!fixture.calls().lines().any(|line| line.starts_with("rm ")));
    }

    #[test]
    fn force_removal_requires_explicit_option() {
        let mut fixture = Fixture::new("running", false);
        smol::block_on(fixture.machine.remove_container("abc123", true)).unwrap();
        assert_eq!(fixture.calls(), "rm --force -- abc123\n");
    }

    #[test]
    fn stopped_removal_does_not_start_or_stop_the_container() {
        let mut fixture = Fixture::new("exited", false);
        smol::block_on(fixture.machine.remove_container("abc123", false)).unwrap();
        let calls = fixture.calls();
        assert_eq!(calls.lines().count(), 2);
        assert!(calls.ends_with("rm -- abc123\n"));
    }
}
