//! Disk discovery dispatched by the selected machine platform.
mod linux;
mod macos;

use crate::machine::{Machine, MachineKind};
use anyhow::{Result, bail};
use utils::disks::{Disk, Disks};

impl Disks for Machine {
    async fn list_disks(&mut self) -> Result<Vec<Disk>> {
        match self.kind {
            MachineKind::Linux => linux::list_disks(self).await,
            MachineKind::MacOS => macos::list_disks(self).await,
            MachineKind::Unknown => bail!("System does not yet support the disks feature"),
        }
    }
}
