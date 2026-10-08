//! One budget belongs to a complete sample, including optional subqueries.
use crate::{machine::Machine, powershell};
use anyhow::{Result, bail};
use std::time::{Duration, Instant};
use utils::{args::Args, output::Output};

const SAMPLE_BUDGET: Duration = Duration::from_secs(30);

pub(crate) struct ResourceCollector<'a> {
    machine: &'a mut Machine,
    deadline: Instant,
}

impl<'a> ResourceCollector<'a> {
    pub fn new(machine: &'a mut Machine) -> Self {
        Self {
            machine,
            deadline: Instant::now() + SAMPLE_BUDGET,
        }
    }

    pub fn check_budget(&self) -> Result<()> {
        if Instant::now() >= self.deadline {
            bail!("Resource sampling timed out");
        }
        Ok(())
    }

    /// The transport, rather than a display name or platform, identifies local sampling.
    #[cfg(target_os = "macos")]
    pub fn is_local(&self) -> bool {
        self.machine.remote.is_none()
    }

    pub async fn run(&mut self, program: &str, args: &Args) -> Result<Output> {
        self.check_budget()?;
        let result = self.machine.run_until(program, args, self.deadline).await;
        self.check_budget()?;
        result
    }

    pub async fn powershell(&mut self, script: &str) -> Result<Output> {
        self.check_budget()?;
        let result = powershell::run_until(self.machine, script, self.deadline).await;
        self.check_budget()?;
        result
    }

    pub async fn delay(&self, duration: Duration) -> Result<()> {
        self.check_budget()?;
        smol::future::race(
            async {
                smol::Timer::after(duration).await;
            },
            async {
                smol::Timer::at(self.deadline).await;
            },
        )
        .await;
        self.check_budget()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;
    use crate::remote_connection::RemoteConnection;

    #[test]
    fn local_native_sampling_requires_no_remote_transport() {
        let mut local = Machine::default();
        local.id = "remote-looking@hostname".into();
        assert!(ResourceCollector::new(&mut local).is_local());

        let mut remote = Machine {
            // Even a disconnected SSH connection to localhost is remote.
            id: "localhost".into(),
            remote: Some(RemoteConnection::default()),
            ..Default::default()
        };
        assert!(!remote.connected());
        assert!(!ResourceCollector::new(&mut remote).is_local());
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use utils::args;

    #[test]
    fn optional_subqueries_share_one_budget_and_expiry_cannot_be_swallowed() -> Result<()> {
        smol::block_on(async {
            let mut machine = Machine::default();
            let mut collector = ResourceCollector {
                machine: &mut machine,
                deadline: Instant::now() + Duration::from_millis(600),
            };
            collector
                .run("sh", &args!["-c", "sleep 0.2; printf first"])
                .await?;
            let optional = collector
                .run("sh", &args!["-c", "exec sleep 10"])
                .await
                .ok();
            assert!(optional.is_none());
            assert!(collector.check_budget().is_err());
            // A fresh per-command budget would incorrectly execute this query.
            let missing = collector
                .run("definitely-not-a-crabdash-command", &Args::new())
                .await;
            assert_eq!(
                missing.err().map(|error| error.to_string()),
                Some("Resource sampling timed out".into())
            );
            Ok(())
        })
    }
}
