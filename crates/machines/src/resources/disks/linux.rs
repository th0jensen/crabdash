//! /proc/diskstats sectors always represent 512 bytes, including 4K-native drives.
use super::DiskCounter;
use std::collections::HashSet;
pub(crate) const SCRIPT: &str = r#"
printf '\n[disks]\n'
for directory in /sys/block/*; do
    [ -e "$directory/device" ] || continue
    printf 'DEVICE\t%s\n' "${directory##*/}"
done
cat /proc/diskstats 2>/dev/null || true
"#;
pub(in crate::resources) fn parse(contents: &str) -> Option<Vec<DiskCounter>> {
    let devices: HashSet<_> = contents
        .lines()
        .filter_map(|line| line.strip_prefix("DEVICE\t"))
        .collect();
    let mut result = Vec::new();
    let mut saw_stats = false;
    for line in contents.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 14
            || fields
                .first()
                .and_then(|value| value.parse::<u32>().ok())
                .is_none()
        {
            continue;
        }
        saw_stats = true;
        if !devices.contains(fields[2]) {
            continue;
        }
        let Some(read_bytes) = fields[5]
            .parse::<u64>()
            .ok()
            .and_then(|value| value.checked_mul(512))
        else {
            continue;
        };
        let Some(written_bytes) = fields[9]
            .parse::<u64>()
            .ok()
            .and_then(|value| value.checked_mul(512))
        else {
            continue;
        };
        result.push(DiskCounter {
            id: fields[2].to_owned(),
            read_bytes,
            written_bytes,
        });
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    saw_stats.then_some(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_whole_devices_avoid_partition_double_count_and_use_512() -> anyhow::Result<()> {
        let result = parse("DEVICE\tnvme0n1\n259 0 nvme0n1 1 0 8 0 2 0 16 0 0 0 0\n259 1 nvme0n1p1 1 0 7 0 2 0 15 0 0 0 0").ok_or_else(|| anyhow::anyhow!("disk"))?;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].read_bytes, 4096);
        assert_eq!(result[0].written_bytes, 8192);
        Ok(())
    }
}
