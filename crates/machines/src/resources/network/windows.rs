//! Raw CIM byte counters, never localized formatted performance-counter paths.
use super::super::NetworkCounter;
use super::super::collector::ResourceCollector;
use anyhow::{Context as _, Result};
use serde::Deserialize;
const SCRIPT: &str = r#"
$rows = @(Get-CimInstance -ClassName Win32_PerfRawData_Tcpip_NetworkInterface -ErrorAction Stop | ForEach-Object {
    if ($null -eq $_.BytesReceivedPerSec -or $null -eq $_.BytesSentPerSec) { throw 'Network byte counters unavailable' }
    [pscustomobject]@{ id = [string]$_.Name; received = ([uint64]$_.BytesReceivedPerSec).ToString(); sent = ([uint64]$_.BytesSentPerSec).ToString() }
})
ConvertTo-Json -InputObject $rows -Depth 3 -Compress
"#;
pub(crate) async fn sample(machine: &mut ResourceCollector<'_>) -> Result<Vec<NetworkCounter>> {
    parse(&machine.powershell(SCRIPT).await?)
}
#[derive(Deserialize)]
struct Row {
    id: String,
    received: String,
    sent: String,
}
fn parse(output: &str) -> Result<Vec<NetworkCounter>> {
    let rows: Vec<Row> = serde_json::from_str(output.trim_start_matches('\u{feff}'))
        .context("Invalid Windows network response")?;
    rows.into_iter()
        .filter(|row| row.id != "_Total" && !row.id.is_empty())
        .map(|row| {
            Ok(NetworkCounter {
                id: row.id,
                received_bytes: row
                    .received
                    .parse()
                    .context("Invalid Windows received byte counter")?,
                sent_bytes: row
                    .sent
                    .parse()
                    .context("Invalid Windows sent byte counter")?,
            })
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_exact_cumulative_counts() -> Result<()> {
        let counters = parse(
            r#"[{"id":"Ethernet","received":"9007199254740993","sent":"128"},{"id":"_Total","received":"1","sent":"1"}]"#,
        )?;
        assert_eq!(counters.len(), 1);
        assert_eq!(counters[0].received_bytes, 9_007_199_254_740_993);
        assert!(parse("[]")?.is_empty());
        assert!(parse(r#"[{"id":"Ethernet","received":"bad","sent":"1"}]"#).is_err());
        assert!(parse(r#"[{"id":"Ethernet","sent":"1"}]"#).is_err());
        assert!(parse(r#"[{"id":"Ethernet","received":null,"sent":"1"}]"#).is_err());
        Ok(())
    }
}
