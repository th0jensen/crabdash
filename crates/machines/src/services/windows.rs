//! Windows Service Control Manager operations and recent lifecycle events.
use crate::{machine::Machine, powershell};
use anyhow::{Result, ensure};
use services::ServiceAction;
use utils::{output::Output, services::ServiceItem};

fn service_lookup(service: &str) -> Result<String> {
    ensure!(!service.trim().is_empty(), "Service name cannot be empty");
    Ok(format!(
        "$serviceName = {}; $service = Get-Service | Where-Object {{ $_.Name -ceq $serviceName }}; if ($null -eq $service) {{ throw 'Service not found' }};",
        powershell::literal(service)?
    ))
}

pub(super) async fn service_action(
    machine: &mut Machine,
    service: &str,
    action: ServiceAction,
) -> Result<Output> {
    let command = match action {
        ServiceAction::Start => "Start-Service",
        ServiceAction::Stop => "Stop-Service",
        ServiceAction::Restart => "Restart-Service",
    };
    powershell::run(
        machine,
        &format!(
            "{} $service | {command} -ErrorAction Stop",
            service_lookup(service)?
        ),
    )
    .await
}

pub(super) async fn service_logs(
    machine: &mut Machine,
    service: &str,
    lines: u32,
) -> Result<Output> {
    let lines = lines.clamp(1, 5000);
    let scan_limit = lines.saturating_mul(20).clamp(200, 10000);
    // SCM events record lifecycle changes for the service. Application logs
    // have no universal mapping from a service name to an event provider.
    let script = format!(
        r#"
        {lookup}
        $events = @()
        try {{
            $events = @(Get-WinEvent -FilterHashtable @{{ LogName = 'System'; ProviderName = 'Service Control Manager' }} -MaxEvents {scan_limit} -ErrorAction Stop)
        }} catch {{
            if ($_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') {{ throw }}
        }}
        $events | Where-Object {{
            $values = @($_.Properties | ForEach-Object {{ [string]$_.Value }})
            $values -contains $service.Name -or $values -contains $service.DisplayName
        }} | Select-Object -First {lines} | Sort-Object TimeCreated | ForEach-Object {{
            '[{{0}}] {{1}} ({{2}}) {{3}}' -f $_.TimeCreated.ToString('yyyy-MM-dd HH:mm:ss'), $_.LevelDisplayName, $_.Id, $_.Message
        }}
    "#,
        lookup = service_lookup(service)?
    );
    powershell::run(machine, &script).await
}

pub(super) async fn list_services(machine: &mut Machine) -> Result<Vec<ServiceItem>> {
    let output = powershell::run(machine, r#"
        $services = @(Get-CimInstance -ClassName Win32_Service | Sort-Object Name | Select-Object Name, DisplayName, State, ProcessId, StartMode, Description)
        ConvertTo-Json -InputObject $services -Depth 3 -Compress
    "#).await?;
    utils::services::windows::parse(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_identifiers_do_not_become_commands_or_wildcards() -> Result<()> {
        let script = service_lookup("service' ; Write-Output hacked #*")?;
        assert!(script.contains("'service'' ; Write-Output hacked #*'"));
        assert!(script.contains("$_.Name -ceq $serviceName"));
        assert!(service_lookup(" ").is_err());
        Ok(())
    }
}
