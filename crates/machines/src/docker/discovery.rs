//! Discover the CLI on the selected machine, independently of daemon state.
pub(super) mod unix;
mod windows;

use crate::machine::{Machine, MachineKind};
use anyhow::Result;

pub(super) async fn find(machine: &mut Machine) -> Result<Option<String>> {
    if (machine.remote.is_some() && matches!(machine.kind, MachineKind::Windows))
        || (machine.remote.is_none() && cfg!(target_os = "windows"))
    {
        windows::find(machine).await
    } else if machine.remote.is_some() {
        unix::remote(machine).await
    } else {
        Ok(unix::local(machine.docker_path.as_deref()))
    }
}
