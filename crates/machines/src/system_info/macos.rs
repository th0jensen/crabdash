//! macOS has no Linux distribution metadata. Compiled on both hosts for SSH.
use super::LinuxDistribution;
use crate::machine::Machine;
use anyhow::Result;

pub(super) async fn distribution(_: &mut Machine) -> Result<Option<LinuxDistribution>> {
    Ok(None)
}
