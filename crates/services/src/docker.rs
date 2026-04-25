use anyhow::Result;
use lucide_icons::Icon;
use utils::{args::Args, container::Container, output::Output};

pub trait Docker {
    /// Finds the Docker executable
    ///
    /// # Returns
    /// * `String`: The Docker binary path
    fn find_docker(&mut self) -> impl Future<Output = String>;
    /// Lists all Docker containers on the machine
    ///
    /// # Returns
    /// * `Ok(Vec<ServiceItem>)`: The containers on the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_docker(&mut self) -> impl Future<Output = Result<Vec<Container>>>;
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
    /// * `args`: The arguments to pass to the Docker command
    ///
    /// # Returns
    /// * `Ok(Output)`: The ID of the container is returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn remove_container(&mut self, id: &str) -> impl Future<Output = Result<Output>>;
    /// Runs an action on a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    /// * `action`: The action to perform ([`DockerAction`])
    ///
    /// # Returns
    /// * `Ok(Output)`: The ID of the container is returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn container_action(&mut self, id: &str, action: &str) -> impl Future<Output = Result<Output>>;
    /// Gets the logs of a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    ///
    /// # Returns
    /// * `Ok(_)`: The container logs are returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn container_logs(&mut self, id: &str) -> impl Future<Output = Result<Output>>;
}

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DockerAction {
    Start,
    Stop,
    Restart,
}

impl DockerAction {
    pub fn command(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::Start => Icon::Play,
            Self::Stop => Icon::X,
            Self::Restart => Icon::RefreshCw,
        }
    }

    pub fn pending_label(self) -> &'static str {
        match self {
            Self::Start => "Starting",
            Self::Stop => "Stopping",
            Self::Restart => "Restarting",
        }
    }
}
