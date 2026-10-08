//! Live resource collection for the selected machine, using its local/SSH transport.
mod collector;
mod disks;
mod gpu;
mod linux;
mod macos;
#[cfg(target_os = "macos")]
mod macos_native;
mod model;
mod network;
mod processes;
mod windows;

pub use disks::{DiskCounter, DiskUsage};
pub use gpu::GpuSample;
pub use network::{NetworkCounter, NetworkUsage};
pub use processes::{ProcessCpu, ProcessSample, ProcessUsage, ProcessesSample};

pub use model::{
    CpuCoreCounter, CpuCounter, CpuSample, CpuUsage, MemorySample, ResourceMonitor, ResourceSample,
    ResourceUsage, SwapSample,
};

use crate::machine::{Machine, MachineKind};
use anyhow::{Result, bail};
use collector::ResourceCollector;

impl Machine {
    pub async fn sample_resources(&mut self) -> Result<ResourceSample> {
        let kind = self.kind;
        let mut collector = ResourceCollector::new(self);
        let sample = match kind {
            MachineKind::Linux => linux::sample(&mut collector).await?,
            MachineKind::MacOS => macos::sample(&mut collector).await?,
            MachineKind::Windows => windows::sample(&mut collector).await?,
            MachineKind::Unknown => {
                bail!("Machine platform is not yet known; refresh the machine first.")
            }
        };
        // Optional domains intentionally tolerate unsupported telemetry, but
        // they cannot turn exhaustion of the shared deadline into success.
        collector.check_budget()?;
        sample.validate()?;
        collector.check_budget()?;
        Ok(sample)
    }
}

/// Fixed section names delimit output from static collector scripts. Neither
/// command text nor metric labels are evaluated as executable code.
fn section<'a>(output: &'a str, name: &str) -> Result<&'a str> {
    let marker = format!("[{name}]\n");
    let (_, content) = output
        .split_once(&marker)
        .ok_or_else(|| anyhow::anyhow!("Missing resource section: {name}"))?;
    Ok(content
        .split_once("\n[")
        .map_or(content, |(value, _)| value)
        .trim())
}
