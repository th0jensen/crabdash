use anyhow::Result;

use crate::ServiceItem;

pub trait Docker {
    const DOCKER_CMD: &str;
    /// Lists all Docker containers on the machine
    ///
    /// # Returns
    /// * `Ok(Vec<ServiceItem>)`: The containers on the machine
    /// * `Err(anyhow::Error)`: Any errors that occurred
    fn list_docker(&mut self) -> Result<Vec<ServiceItem>>;
    /// Runs an action on a Docker container
    ///
    /// # Arguments
    /// * `id`: The container ID
    /// * `action`: The action to perform (start/stop/restart)
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
