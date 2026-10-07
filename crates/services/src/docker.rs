use anyhow::Result;
use lucide_icons::Icon;
use utils::{
    args::Args,
    container::{Container, details::ContainerDetails},
    output::Output,
};

pub trait Docker {
    /// Finds the Docker executable
    ///
    /// # Returns
    /// * `Ok(String)`: The Docker binary path
    /// * `Err(DockerNotInstalled)`: No Docker executable is installed
    fn find_docker(&mut self) -> impl Future<Output = Result<String>>;
    /// Lists all Docker containers on the machine
    ///
    /// # Returns
    /// * `Ok(Vec<ServiceItem>)`: The containers on the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_docker(&mut self) -> impl Future<Output = Result<Vec<Container>>>;
    /// Reads configuration and current state for a single container.
    fn inspect_container(&mut self, id: &str) -> impl Future<Output = Result<ContainerDetails>>;
    /// Runs a Docker container
    ///
    /// # Arguments
    /// * `args`: The arguments to pass to the Docker command
    ///
    /// # Returns
    /// * `Ok(String)`: The ID of the container is returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn run_container(&mut self, args: &Args) -> impl Future<Output = Result<Output>>;
    /// Removes a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    /// * `force`: Kill and remove directly instead of stopping gracefully
    ///
    /// Images and volumes are preserved.
    ///
    /// # Returns
    /// * `Ok(Output)`: The ID of the container is returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn remove_container(&mut self, id: &str, force: bool) -> impl Future<Output = Result<Output>>;
    /// Runs an action on a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    /// * `action`: The action to perform ([`DockerAction`])
    ///
    /// # Returns
    /// * `Ok(Output)`: The ID of the container is returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn container_action(
        &mut self,
        id: &str,
        action: DockerAction,
    ) -> impl Future<Output = Result<Output>>;
    /// Gets the logs of a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    ///
    /// # Returns
    /// * `Ok(_)`: The container logs are returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn container_logs(&mut self, id: &str, lines: u32) -> impl Future<Output = Result<Output>>;
}

/// A missing executable is an installation state, distinct from daemon,
/// permission, transport, and container failures.
#[derive(Debug)]
pub struct DockerNotInstalled;

impl std::fmt::Display for DockerNotInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Docker not installed")
    }
}

impl std::error::Error for DockerNotInstalled {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RestartPolicy {
    #[default]
    No,
    Always,
    OnFailure,
    UnlessStopped,
}

impl RestartPolicy {
    pub fn flag_value(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::Always => "always",
            Self::OnFailure => "on-failure",
            Self::UnlessStopped => "unless-stopped",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::No => "No",
            Self::Always => "Always",
            Self::OnFailure => "On Failure",
            Self::UnlessStopped => "Unless Stopped",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::No, Self::Always, Self::OnFailure, Self::UnlessStopped]
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NetworkMode {
    #[default]
    Bridge,
    Host,
    None,
}

impl NetworkMode {
    pub fn flag_value(self) -> Option<&'static str> {
        match self {
            Self::Bridge => Option::None,
            Self::Host => Some("host"),
            Self::None => Some("none"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bridge => "Bridge",
            Self::Host => "Host",
            Self::None => "None",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Bridge, Self::Host, Self::None]
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DockerFilter {
    #[default]
    Total,
    Running,
    Paused,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DockerAction {
    Start,
    Stop,
    Restart,
    Pause,
    Unpause,
    Remove { force: bool },
}

impl DockerAction {
    pub fn command(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Pause => "pause",
            Self::Unpause => "unpause",
            Self::Remove { .. } => "rm",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::Start => Icon::Play,
            Self::Stop => Icon::Square,
            Self::Restart => Icon::RefreshCw,
            Self::Pause => Icon::Pause,
            Self::Unpause => Icon::Play,
            Self::Remove { .. } => Icon::Trash2,
        }
    }

    pub fn pending_label(self) -> &'static str {
        match self {
            Self::Start => "Starting",
            Self::Stop => "Stopping",
            Self::Restart => "Restarting",
            Self::Pause => "Pausing",
            Self::Unpause => "Resuming",
            Self::Remove { .. } => "Removing",
        }
    }
}

impl DockerAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Stop => "Stop",
            Self::Restart => "Restart",
            Self::Pause => "Pause",
            Self::Unpause => "Resume",
            Self::Remove { .. } => "Remove",
        }
    }

    pub fn allowed_for(self, container: &Container) -> bool {
        match self {
            Self::Start => matches!(container.status.as_str(), "created" | "exited"),
            Self::Stop | Self::Restart => container.is_running_status(),
            Self::Pause => container.is_running_status(),
            Self::Unpause => container.is_paused(),
            Self::Remove { .. } => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_follow_container_state() {
        for (state, allowed) in [
            (
                "running",
                vec![
                    DockerAction::Stop,
                    DockerAction::Restart,
                    DockerAction::Pause,
                ],
            ),
            ("paused", vec![DockerAction::Unpause]),
            ("exited", vec![DockerAction::Start]),
            ("created", vec![DockerAction::Start]),
            ("restarting", vec![]),
            ("dead", vec![]),
        ] {
            let container = Container {
                status: state.into(),
                ..Default::default()
            };
            for action in [
                DockerAction::Start,
                DockerAction::Stop,
                DockerAction::Restart,
                DockerAction::Pause,
                DockerAction::Unpause,
            ] {
                assert_eq!(
                    action.allowed_for(&container),
                    allowed.contains(&action),
                    "{state}: {action:?}"
                );
            }
        }
    }
}
