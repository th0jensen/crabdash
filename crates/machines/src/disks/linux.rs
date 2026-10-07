use crate::machine::Machine;
use anyhow::Result;
use utils::{args, args::Args, disks::Disk};

pub(super) async fn list_disks(machine: &mut Machine) -> Result<Vec<Disk>> {
    let stdout = machine
        .run(
            "lsblk",
            &args![
                "-P",
                "-o",
                "NAME,PATH,SIZE,TYPE,MOUNTPOINTS,MODEL,PKNAME,FSTYPE,LABEL,RM,HOTPLUG,TRAN"
            ],
        )
        .await?;
    Ok(utils::disks::linux::parse(stdout))
}
