//! Linux procfs snapshots work unchanged for localhost, SSH, and WSL targets.
use super::{
    CpuCoreCounter, CpuCounter, CpuSample, MemorySample, ResourceSample, SwapSample, section,
};
use crate::machine::Machine;
use anyhow::{Context as _, Result, ensure};
use std::collections::HashMap;
use utils::{args, args::Args};

const SCRIPT: &str = r#"set -eu
export LC_ALL=C
printf '[cpu]\n'; cat /proc/stat
printf '\n[memory]\n'; cat /proc/meminfo
printf '\n[uptime]\n'; cat /proc/uptime
printf '\n[load]\n'; cat /proc/loadavg 2>/dev/null || true
printf '\n[boot]\n'; cat /proc/sys/kernel/random/boot_id 2>/dev/null || true
"#;

pub(super) async fn sample(machine: &mut Machine) -> Result<ResourceSample> {
    let script = [
        SCRIPT,
        super::processes::linux::SCRIPT,
        super::network::linux::SCRIPT,
        super::disks::linux::SCRIPT,
        super::gpu::linux::SCRIPT,
    ]
    .concat();
    let captured_at = std::time::Instant::now();
    let output = machine.run("sh", &args!["-c", script]).await?;
    let mut sample = parse(&output)?;
    sample.captured_at = captured_at;
    Ok(sample)
}

fn parse(output: &str) -> Result<ResourceSample> {
    let stat = section(output, "cpu")?;
    let mut aggregate = None;
    let mut cores = Vec::new();
    let mut boot_time = None;
    for line in stat.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else {
            continue;
        };
        if name == "btime" {
            boot_time = fields.next();
        }
        if name != "cpu"
            && !name
                .strip_prefix("cpu")
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|value| value.is_ascii_digit()))
        {
            continue;
        }
        // Guest/guest_nice (fields 9/10) are already included in user/nice.
        let times = fields
            .take(8)
            .map(str::parse::<u64>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Invalid Linux CPU counter")?;
        ensure!(times.len() >= 4, "Incomplete Linux CPU counter");
        let total = times.iter().try_fold(0_u64, |sum, value| {
            sum.checked_add(*value)
                .context("Linux CPU counter overflow")
        })?;
        let idle = times[3]
            .checked_add(times.get(4).copied().unwrap_or(0))
            .context("Linux idle counter overflow")?;
        let counter = CpuCounter { total, idle };
        if name == "cpu" {
            aggregate = Some(counter);
        } else {
            cores.push(CpuCoreCounter {
                name: name.trim_start_matches("cpu").to_owned(),
                counter,
            });
        }
    }
    cores.sort_by_key(|core| core.name.parse::<u32>().unwrap_or(u32::MAX));
    let memory = parse_memory(section(output, "memory")?)?;
    let uptime_seconds = section(output, "uptime")?
        .split_whitespace()
        .next()
        .context("Missing Linux uptime")?
        .parse()
        .context("Invalid Linux uptime")?;
    let load = section(output, "load")?;
    let load_average = if load.is_empty() {
        None
    } else {
        let mut fields = load.split_whitespace();
        Some([
            fields
                .next()
                .context("Missing one-minute load")?
                .parse()
                .context("Invalid load average")?,
            fields
                .next()
                .context("Missing five-minute load")?
                .parse()
                .context("Invalid load average")?,
            fields
                .next()
                .context("Missing fifteen-minute load")?
                .parse()
                .context("Invalid load average")?,
        ])
    };
    let boot = section(output, "boot")?;
    let boot_id = if boot.is_empty() {
        format!(
            "linux:{}",
            boot_time.context("Missing Linux boot identity")?
        )
    } else {
        boot.to_owned()
    };
    let logical_cpus = cores.len();
    let aggregate = aggregate.context("Missing Linux aggregate CPU counter")?;
    let sample = ResourceSample {
        cpu: CpuSample::Counters { aggregate, cores },
        logical_cpus,
        memory: memory.0,
        swap: memory.1,
        load_average,
        uptime_seconds,
        boot_id,
        captured_at: std::time::Instant::now(),
        processes: section(output, "processes")
            .ok()
            .and_then(|value| super::processes::linux::parse(value, aggregate.total)),
        network: section(output, "network")
            .ok()
            .and_then(super::network::linux::parse),
        disks: section(output, "disks")
            .ok()
            .and_then(super::disks::linux::parse),
        gpus: section(output, "gpus")
            .ok()
            .and_then(|value| super::gpu::linux::parse(value, section(output, "nvidia").ok())),
    };
    sample.validate()?;
    Ok(sample)
}

fn parse_memory(contents: &str) -> Result<(MemorySample, Option<SwapSample>)> {
    let mut fields = HashMap::new();
    for line in contents.lines() {
        let Some((name, values)) = line.split_once(':') else {
            continue;
        };
        if !matches!(
            name,
            "MemTotal"
                | "MemAvailable"
                | "MemFree"
                | "Buffers"
                | "Cached"
                | "SReclaimable"
                | "Shmem"
                | "SwapTotal"
                | "SwapFree"
        ) {
            continue;
        }
        let mut values = values.split_whitespace();
        let value = values
            .next()
            .context("Missing Linux memory value")?
            .parse::<u64>()
            .context("Invalid Linux memory value")?;
        ensure!(values.next() == Some("kB"), "Unexpected Linux memory unit");
        let bytes = value
            .checked_mul(1024)
            .context("Linux memory value overflow")?;
        fields.insert(name, bytes);
    }
    let total_bytes = *fields
        .get("MemTotal")
        .context("Missing Linux memory total")?;
    let (available_bytes, estimated) = match fields.get("MemAvailable") {
        Some(value) => (*value, false),
        None => (
            fields
                .get("MemFree")
                .copied()
                .context("Missing Linux free memory")?
                .saturating_add(fields.get("Buffers").copied().unwrap_or(0))
                .saturating_add(fields.get("Cached").copied().unwrap_or(0))
                .saturating_add(fields.get("SReclaimable").copied().unwrap_or(0))
                .saturating_sub(fields.get("Shmem").copied().unwrap_or(0)),
            true,
        ),
    };
    let swap = match (fields.get("SwapTotal"), fields.get("SwapFree")) {
        (Some(total), Some(free)) => Some(SwapSample {
            total_bytes: *total,
            free_bytes: *free,
        }),
        _ => None,
    };
    Ok((
        MemorySample {
            total_bytes,
            available_bytes: available_bytes.min(total_bytes),
            estimated,
        },
        swap,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guest_ticks_are_not_counted_twice_and_memory_is_bytes() -> Result<()> {
        let output = "[cpu]\ncpu 10 2 3 100 5 6 7 8 9 1\ncpu0 10 2 3 100 5 6 7 8 9 1\nbtime 1700000000\n[memory]\nMemTotal: 1000 kB\nMemAvailable: 250 kB\nSwapTotal: 500 kB\nSwapFree: 300 kB\n[uptime]\n120.5 80.0\n[load]\n0.25 0.5 1.5 1/200 100\n[boot]\n";
        let sample = parse(output)?;
        let CpuSample::Counters { aggregate, .. } = sample.cpu else {
            anyhow::bail!("Expected counters");
        };
        assert_eq!(
            aggregate,
            CpuCounter {
                total: 141,
                idle: 105
            }
        );
        assert_eq!(sample.memory.used_bytes(), 750 * 1024);
        assert_eq!(sample.load_average, Some([0.25, 0.5, 1.5]));
        assert_eq!(sample.uptime_seconds, 120.5);
        assert_eq!(sample.boot_id, "linux:1700000000");
        assert_eq!(
            sample.swap.context("Expected swap")?.used_bytes(),
            200 * 1024
        );
        Ok(())
    }
    #[test]
    fn old_kernel_memory_fallback_is_explicitly_estimated() -> Result<()> {
        let (memory, _) = parse_memory(
            "MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 20 kB\nCached: 200 kB\nSReclaimable: 10 kB\nShmem: 30 kB\n",
        )?;
        assert_eq!(memory.available_bytes, 300 * 1024);
        assert!(memory.estimated);
        assert!(parse_memory("MemTotal: 1000 MB\nMemFree: 1 kB").is_err());
        assert!(parse_memory("MemTotal: 18446744073709551615 kB\nMemFree: 1 kB").is_err());
        assert!(parse("[cpu]\ncpu 1 2\n").is_err());
        Ok(())
    }
}
