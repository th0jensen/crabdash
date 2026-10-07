use crate::machine::Machine;
use anyhow::Result;
use services::ServiceAction;
use utils::{args, args::Args, output::Output, services::ServiceItem};

pub(super) async fn service_action(
    machine: &mut Machine,
    service: &str,
    action: ServiceAction,
) -> Result<Output> {
    machine
        .run("systemctl", &args![action.command(), "--", service])
        .await
}
pub(super) async fn service_logs(
    machine: &mut Machine,
    service: &str,
    lines: u32,
) -> Result<Output> {
    machine
        .run(
            "journalctl",
            &args![
                "--unit",
                service,
                "--no-pager",
                "--quiet",
                "--lines",
                &lines.to_string(),
                "--output",
                "short-iso"
            ],
        )
        .await
}
pub(super) async fn list_services(machine: &mut Machine) -> Result<Vec<ServiceItem>> {
    // Pass the glob literally so systemctl expands it against loaded units,
    // including inactive ones, rather than the local or remote shell's files.
    let output = machine
        .run(
            "systemctl",
            &args![
                "show",
                "--all",
                "--no-pager",
                "--property=Id,Description,LoadState,ActiveState,SubState,MainPID,UnitFileState",
                "--",
                "*.service"
            ],
        )
        .await?;
    utils::services::linux::parse(&output)
}
