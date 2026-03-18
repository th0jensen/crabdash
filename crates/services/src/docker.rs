use anyhow::Result;
use lucide_icons::Icon;

#[derive(Clone, Debug, Default)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}

impl Container {
    pub fn is_running_status(&self) -> bool {
        let normalized = self.status.to_ascii_lowercase();
        normalized.contains("running") || normalized.contains("healthy")
    }

    pub fn parse_output(stdout: String) -> Vec<Container> {
        stdout
            .lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');

                let id = parts.next()?.to_string();
                let name = parts.next()?.to_string();
                let state = parts.next()?.to_string();

                Some(Container {
                    id,
                    name,
                    status: state,
                    error: None,
                })
            })
            .collect()
    }
}

pub trait Docker {
    /// Finds the Docker executable
    ///
    /// # Returns
    /// * `String`: The Docker binary path
    fn find_docker(&mut self) -> String;
    /// Lists all Docker containers on the machine
    ///
    /// # Returns
    /// * `Ok(Vec<ServiceItem>)`: The containers on the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_docker(&mut self) -> Result<Vec<Container>>;
    /// Runs an action on a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    /// * `action`: The action to perform ([`DockerAction`])
    ///
    /// # Returns
    /// * `Ok(String)`: The ID of the container is returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn container_action(&mut self, id: &str, action: &str) -> Result<String>;
    /// Gets the logs of a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    ///
    /// # Returns
    /// * `Ok(_)`: The container logs are returned
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn container_logs(&mut self, id: &str) -> Result<String>;
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
