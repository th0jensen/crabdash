//! System service management dispatched by the selected machine platform.
mod linux;
mod macos;
mod windows;

use crate::machine::{Machine, MachineKind};
use anyhow::{Result, bail};
use services::{ServiceAction, Services};
use utils::{output::Output, services::ServiceItem};

impl Services for Machine {
    async fn service_action(&mut self, service: &str, action: ServiceAction) -> Result<Output> {
        match self.kind {
            MachineKind::Linux => linux::service_action(self, service, action).await,
            MachineKind::MacOS => macos::service_action(self, service, action).await,
            MachineKind::Windows => windows::service_action(self, service, action).await,
            MachineKind::Unknown => bail!("System does not yet support the services feature"),
        }
    }
    async fn service_logs(&mut self, service: &str, lines: u32) -> Result<Output> {
        match self.kind {
            MachineKind::Linux => linux::service_logs(self, service, lines).await,
            MachineKind::MacOS => macos::service_logs(self, service, lines).await,
            MachineKind::Windows => windows::service_logs(self, service, lines).await,
            MachineKind::Unknown => bail!("System does not yet support the services feature"),
        }
    }
    async fn list_services(&mut self) -> Result<Vec<ServiceItem>> {
        match self.kind {
            MachineKind::Linux => linux::list_services(self).await,
            MachineKind::MacOS => macos::list_services(self).await,
            MachineKind::Windows => windows::list_services(self).await,
            MachineKind::Unknown => bail!("System does not yet support the services feature"),
        }
    }
}
