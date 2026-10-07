//! Windows Storage cmdlets through the local/SSH command transport.
use crate::{machine::Machine, powershell};
use anyhow::Result;
use utils::disks::Disk;

pub(super) async fn list_disks(machine: &mut Machine) -> Result<Vec<Disk>> {
    let output = powershell::run(machine, r#"
        $disks = @(Get-Disk | Sort-Object Number | ForEach-Object {
            $disk = $_
            $partitions = @(Get-Partition -DiskNumber $disk.Number | Sort-Object PartitionNumber | ForEach-Object {
                $partition = $_
                $volume = $null
                try { $volume = $partition | Get-Volume -ErrorAction Stop } catch { }
                [pscustomobject]@{
                    Number = $partition.PartitionNumber
                    Size = $partition.Size
                    Type = [string]$partition.Type
                    Paths = @($partition.AccessPaths | Where-Object { $_ })
                    FileSystem = [string]$volume.FileSystem
                    Label = [string]$volume.FileSystemLabel
                }
            })
            [pscustomobject]@{
                Number = $disk.Number
                Name = [string]$disk.FriendlyName
                Size = $disk.Size
                Health = [string]$disk.HealthStatus
                Offline = $disk.IsOffline
                Bus = [string]$disk.BusType
                Style = [string]$disk.PartitionStyle
                Partitions = $partitions
            }
        })
        ConvertTo-Json -InputObject $disks -Depth 5 -Compress
    "#).await?;
    utils::disks::windows::parse(&output)
}
