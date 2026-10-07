//! Physical disks expose cumulative byte counts despite their PerSec names.
use super::super::DiskCounter;
use crate::{machine::Machine, powershell};
use anyhow::{Context as _, Result};
use serde::Deserialize;
const SCRIPT: &str = r#"
$rows = @(Get-CimInstance -ClassName Win32_PerfRawData_PerfDisk_PhysicalDisk -ErrorAction Stop | ForEach-Object {
    if ($null -eq $_.DiskReadBytesPerSec -or $null -eq $_.DiskWriteBytesPerSec) { throw 'Disk byte counters unavailable' }
    [pscustomobject]@{ id = [string]$_.Name; read = ([uint64]$_.DiskReadBytesPerSec).ToString(); written = ([uint64]$_.DiskWriteBytesPerSec).ToString() }
})
ConvertTo-Json -InputObject $rows -Depth 3 -Compress
"#;
pub(crate) async fn sample(machine: &mut Machine) -> Result<Vec<DiskCounter>> {
    parse(&powershell::run(machine, SCRIPT).await?)
}
#[derive(Deserialize)]
struct Row {
    id: String,
    read: String,
    written: String,
}
fn parse(output: &str) -> Result<Vec<DiskCounter>> {
    let rows: Vec<Row> = serde_json::from_str(output.trim_start_matches('\u{feff}'))
        .context("Invalid Windows disk response")?;
    rows.into_iter()
        .filter(|row| row.id != "_Total" && !row.id.is_empty())
        .map(|row| {
            Ok(DiskCounter {
                id: row.id,
                read_bytes: row
                    .read
                    .parse()
                    .context("Invalid Windows disk read byte counter")?,
                written_bytes: row
                    .written
                    .parse()
                    .context("Invalid Windows disk write byte counter")?,
            })
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_total_without_losing_disk_identity() -> Result<()> {
        let values = parse(
            r#"[{"id":"0 C:","read":"9007199254740993","written":"10"},{"id":"1 D:","read":"2","written":"3"},{"id":"_Total","read":"4","written":"5"}]"#,
        )?;
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].read_bytes, 9_007_199_254_740_993);
        assert_eq!(values[1].id, "1 D:");
        assert!(parse("[]")?.is_empty());
        assert!(parse(r#"[{"id":"0 C:","written":"1"}]"#).is_err());
        assert!(parse(r#"[{"id":"0 C:","read":"1","written":null}]"#).is_err());
        Ok(())
    }
}
