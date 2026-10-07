use crate::machine::Machine;
use anyhow::Result;
use services::ServiceAction;
use utils::{args, args::Args, output::Output, services::ServiceItem};

pub(super) async fn service_action(
    machine: &mut Machine,
    service: &str,
    action: ServiceAction,
) -> Result<Output> {
    match action {
        ServiceAction::Start => machine.run("launchctl", &args!["start", service]).await,
        ServiceAction::Stop => machine.run("launchctl", &args!["stop", service]).await,
        ServiceAction::Restart => {
            machine.run("launchctl", &args!["stop", service]).await?;
            machine.run("launchctl", &args!["start", service]).await
        }
    }
}
pub(super) async fn service_logs(
    machine: &mut Machine,
    service: &str,
    lines: u32,
) -> Result<Output> {
    let mut output = machine
        .run(
            "log",
            &args![
                "show",
                "--style",
                "compact",
                "--last",
                "1h",
                "--predicate",
                &format!("process == '{service}' OR subsystem == '{service}'")
            ],
        )
        .await?;
    // macOS log show has no line-count option; retain the newest requested lines.
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(output.0.iter().enumerate().filter_map(|(index, byte)| {
            (*byte == b'\n' && index + 1 < output.0.len()).then_some(index + 1)
        }))
        .collect();
    if let Some(start) = line_starts.get(line_starts.len().saturating_sub(lines as usize)) {
        output.0.drain(..*start);
    }

    Ok(output)
}
pub(super) async fn list_services(machine: &mut Machine) -> Result<Vec<ServiceItem>> {
    Ok(utils::services::macos::parse(
        &machine.run("launchctl", &args!["list"]).await?,
    ))
}
