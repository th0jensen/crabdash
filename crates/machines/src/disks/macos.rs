use crate::machine::Machine;
use anyhow::Result;
use utils::{args, args::Args, disks::Disk};

pub(super) async fn list_disks(machine: &mut Machine) -> Result<Vec<Disk>> {
    let list_stdout = machine.run("diskutil", &args!["list", "-plist"]).await?;
    let apfs_stdout = machine
        .run("diskutil", &args!["apfs", "list", "-plist"])
        .await
        .ok();
    let mut disks = utils::disks::macos::parse(list_stdout, apfs_stdout)?;

    for disk in &mut disks {
        let identifier = disk.id.trim_start_matches("/dev/");
        if let Ok(info_stdout) = machine
            .run("diskutil", &args!["info", "-plist", identifier])
            .await
        {
            let _ = disk.apply_diskutil_info(&info_stdout);
        }
    }

    Ok(disks)
}
