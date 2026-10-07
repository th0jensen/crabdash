use crate::machine::Machine;
use anyhow::Result;
use indoc::indoc;
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
    Ok(utils::services::linux::parse(&machine.run("sh", &args!["-c", indoc! {r#"
                            units=$(LC_ALL=C systemctl list-units --type=service --all --no-legend --no-pager --plain) || exit $?
                            if [ -n "$units" ]; then
                                printf '%s\n' "$units" | while read -r unit load active sub description; do
                                    pid=$(systemctl show --property=MainPID --value "$unit") || exit $?
                                    unit_file_state=$(systemctl show --property=UnitFileState --value "$unit") || exit $?
                                    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$pid" "$load" "$active" "$sub" "$unit_file_state" "$unit" "$description"
                                done
                            fi
                        "#}]).await?))
}
