//! Built-in macOS commands; also compiled on Linux/Windows for SSH targets.
use super::collector::ResourceCollector;
use super::{CpuSample, MemorySample, ResourceSample, SwapSample, section};
use anyhow::{Context as _, Result, ensure};
use std::collections::HashMap;
use utils::{args, args::Args};

// top's first sample has no usable interval. Two logging samples produce a
// target-side interval without installing a helper or assuming Python exists.
const SCRIPT: &str = r#"set -eu
export LC_ALL=C
printf '[cpu]\n'; /usr/bin/top -l 2 -s 1 -n 0 -F -R
printf '\n[memory]\n'; /usr/bin/vm_stat
printf '\n[total]\n'; /usr/sbin/sysctl -n hw.memsize
printf '\n[cores]\n'; /usr/sbin/sysctl -n hw.logicalcpu
printf '\n[boot]\n'; /usr/sbin/sysctl -n kern.boottime
printf '\n[time]\n'; /bin/date +%s
printf '\n[swap]\n'; /usr/sbin/sysctl -n vm.swapusage 2>/dev/null || true
"#;

pub(super) async fn sample(machine: &mut ResourceCollector<'_>) -> Result<ResourceSample> {
    let captured_at = std::time::Instant::now();
    let output = machine.run("sh", &args!["-c", SCRIPT]).await?;
    let mut sample = parse(&output)?;
    sample.processes = super::processes::macos::sample(machine).await.ok();
    sample.network = super::network::macos::sample(machine).await.ok();
    sample.disks = super::disks::macos::sample(machine).await.ok();
    sample.gpus = super::gpu::macos::sample(machine).await.ok();
    sample.captured_at = captured_at;
    Ok(sample)
}

fn parse(output: &str) -> Result<ResourceSample> {
    let top = section(output, "cpu")?;
    let cpu_lines = top
        .lines()
        .filter_map(|line| line.trim().strip_prefix("CPU usage:"))
        .collect::<Vec<_>>();
    ensure!(cpu_lines.len() >= 2, "Missing second macOS CPU interval");
    let cpu_line = cpu_lines.last().context("Missing macOS CPU interval")?;
    let idle = cpu_line
        .split(',')
        .find(|value| value.contains("idle"))
        .context("Missing macOS idle CPU")?
        .trim()
        .split_once('%')
        .context("Invalid macOS idle CPU")?
        .0
        .trim()
        .parse::<f64>()
        .context("Invalid macOS idle CPU")?;
    ensure!(
        idle.is_finite() && (0.0..=100.0).contains(&idle),
        "Invalid macOS idle percentage"
    );
    let total_bytes = section(output, "total")?
        .parse::<u64>()
        .context("Invalid macOS total memory")?;
    let memory = parse_memory(section(output, "memory")?, total_bytes)?;
    let logical_cpus = section(output, "cores")?
        .parse()
        .context("Invalid macOS logical CPU count")?;
    let boot = section(output, "boot")?;
    let boot_seconds = boot
        .split_once("sec =")
        .context("Invalid macOS boot time")?
        .1
        .split(',')
        .next()
        .context("Missing macOS boot time")?
        .trim()
        .parse::<u64>()
        .context("Invalid macOS boot time")?;
    let now = section(output, "time")?
        .parse::<u64>()
        .context("Invalid macOS sample time")?;
    let uptime_seconds = now
        .checked_sub(boot_seconds)
        .context("macOS clock is earlier than boot time")? as f64;
    let load_average = top
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Load Avg:"))
        .last()
        .map(|line| {
            let fields = line
                .split(',')
                .map(str::trim)
                .map(str::parse::<f64>)
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("Invalid macOS load average")?;
            ensure!(fields.len() == 3, "Incomplete macOS load average");
            Ok::<_, anyhow::Error>([fields[0], fields[1], fields[2]])
        })
        .transpose()?;
    let swap = parse_swap(section(output, "swap")?).ok();
    let sample = ResourceSample {
        cpu: CpuSample::Sampled {
            percent: 100.0 - idle,
        },
        logical_cpus,
        memory,
        swap,
        load_average,
        uptime_seconds,
        boot_id: format!("macos:{boot_seconds}"),
        captured_at: std::time::Instant::now(),
        processes: None,
        network: None,
        disks: None,
        gpus: None,
    };
    sample.validate()?;
    Ok(sample)
}

fn parse_memory(output: &str, total_bytes: u64) -> Result<MemorySample> {
    let header = output
        .lines()
        .next()
        .context("Missing macOS VM statistics")?;
    let page_size = header
        .split_once("page size of ")
        .context("Missing macOS VM page size")?
        .1
        .split_whitespace()
        .next()
        .context("Missing macOS VM page size")?
        .parse::<u64>()
        .context("Invalid macOS VM page size")?;
    ensure!(page_size > 0, "Invalid macOS VM page size");
    let mut pages = HashMap::new();
    for line in output.lines().skip(1) {
        let Some((name, count)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim_matches('"');
        if !matches!(name, "Pages free" | "Pages inactive") {
            continue;
        }
        pages.insert(
            name,
            count
                .trim()
                .trim_end_matches('.')
                .parse::<u64>()
                .context("Invalid macOS VM page count")?,
        );
    }
    // free_count already includes speculative pages. Purgeable pages can
    // overlap inactive pages; compressor pages are a separate resident count.
    // Neither is added to or subtracted from this free-plus-inactive estimate.
    let available_pages = *pages
        .get("Pages free")
        .context("Missing macOS free pages")?;
    let available_pages = available_pages
        .checked_add(
            *pages
                .get("Pages inactive")
                .context("Missing macOS inactive pages")?,
        )
        .context("macOS available page count overflow")?;
    let available_bytes = available_pages
        .checked_mul(page_size)
        .context("macOS available memory overflow")?
        .min(total_bytes);
    // This is an estimate from VM page categories, not a memory-pressure metric.
    Ok(MemorySample {
        total_bytes,
        available_bytes,
        estimated: true,
    })
}

fn parse_swap(output: &str) -> Result<SwapSample> {
    fn value(output: &str, key: &str) -> Result<u64> {
        let raw = output
            .split_once(key)
            .context("Missing macOS swap value")?
            .1
            .split_whitespace()
            .next()
            .context("Missing macOS swap value")?;
        let (boundary, _) = raw
            .char_indices()
            .next_back()
            .context("Invalid macOS swap value")?;
        let (number, unit) = raw.split_at(boundary);
        let multiplier = match unit {
            "K" => 1024.0,
            "M" => 1048576.0,
            "G" => 1073741824.0,
            "T" => 1099511627776.0,
            _ => anyhow::bail!("Invalid macOS swap unit"),
        };
        let bytes = number.parse::<f64>().context("Invalid macOS swap value")? * multiplier;
        ensure!(
            bytes.is_finite() && bytes >= 0.0 && bytes < u64::MAX as f64,
            "Invalid macOS swap value"
        );
        Ok(bytes.round() as u64)
    }
    let swap = SwapSample {
        total_bytes: value(output, "total = ")?,
        free_bytes: value(output, "free = ")?,
    };
    ensure!(
        swap.free_bytes <= swap.total_bytes,
        "Invalid macOS swap values"
    );
    Ok(swap)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn last_cpu_interval_and_actual_vm_page_size_are_used() -> Result<()> {
        let output = "[cpu]\nCPU usage: 0.0% user, 0.0% sys, 100.0% idle\nLoad Avg: 0.5, 1.0, 1.5\nCPU usage: 15.0% user, 5.0% sys, 80.0% idle\nLoad Avg: 0.6, 1.1, 1.6\n[memory]\nMach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 10.\nPages inactive: 20.\nPages purgeable: 5.\nPages speculative: 6.\nPages stored in compressor: 999.\nPages occupied by compressor: 3.\n[total]\n1048576\n[cores]\n8\n[boot]\n{ sec = 1700000000, usec = 0 } Mon Jan 1\n[time]\n1700000123\n[swap]\ntotal = 1024.00M  used = 59.00M  free = 965.00M (encrypted)\n";
        let sample = parse(output)?;
        assert!(matches!(sample.cpu, CpuSample::Sampled { percent: 20.0 }));
        assert_eq!(sample.logical_cpus, 8);
        assert_eq!(sample.memory.available_bytes, 30 * 16384);
        assert!(sample.memory.estimated);
        assert_eq!(sample.uptime_seconds, 123.0);
        assert_eq!(sample.load_average, Some([0.6, 1.1, 1.6]));
        assert_eq!(
            sample.swap.context("Expected swap")?.total_bytes,
            1024 * 1048576
        );
        Ok(())
    }
    #[test]
    fn compressor_larger_than_free_and_inactive_does_not_erase_available_memory() -> Result<()> {
        let output = "VM (page size of 16384 bytes)\nPages free: 4228.\nPages inactive: 190493.\nPages purgeable: 9000.\nPages occupied by compressor: 448199.\n";
        let memory = parse_memory(output, 16 * 1024 * 1024 * 1024)?;
        assert_eq!(memory.available_bytes, (4228 + 190493) * 16384);
        assert!(memory.available_bytes > 0);
        assert!(memory.estimated);
        Ok(())
    }

    #[test]
    fn other_vm_categories_do_not_change_estimate_and_missing_optional_counts_are_allowed()
    -> Result<()> {
        let base = "VM (page size of 4096 bytes)\nPages free: 10.\nPages inactive: 20.\n";
        let expected = parse_memory(base, 1024 * 1024)?.available_bytes;
        for extra in [
            "Pages purgeable: 500.\nPages speculative: 700.\nPages occupied by compressor: 900.\nPages stored in compressor: 999.\n",
            "Pages purgeable: 0.\nPages speculative: 0.\nPages occupied by compressor: 0.\n",
            "Pages purgeable: unavailable.\nPages occupied by compressor: unavailable.\n",
        ] {
            assert_eq!(
                parse_memory(&format!("{base}{extra}"), 1024 * 1024)?.available_bytes,
                expected
            );
        }
        assert_eq!(expected, 30 * 4096);
        Ok(())
    }

    #[test]
    fn memory_estimate_requires_both_counts_checks_overflow_and_caps_at_total() -> Result<()> {
        for output in [
            "VM (page size of 4096 bytes)\nPages free: 10.\n",
            "VM (page size of 4096 bytes)\nPages inactive: 20.\n",
            "VM (page size of 4096 bytes)\nPages free: invalid.\nPages inactive: 20.\n",
            "VM (page size of 4096 bytes)\nPages free: 18446744073709551615.\nPages inactive: 1.\n",
            "VM (page size of 4096 bytes)\nPages free: 4503599627370496.\nPages inactive: 0.\n",
        ] {
            assert!(parse_memory(output, u64::MAX).is_err());
        }
        for page_size in [4096, 16384] {
            let output = format!(
                "VM (page size of {page_size} bytes)\nPages free: 10.\nPages inactive: 20.\n"
            );
            assert_eq!(
                parse_memory(&output, u64::MAX)?.available_bytes,
                30 * page_size
            );
            assert_eq!(parse_memory(&output, 65536)?.available_bytes, 65536);
        }
        Ok(())
    }
    #[test]
    fn malformed_vm_and_swap_data_do_not_panic() {
        assert!(parse_memory("no page size\nPages free: 1", 1024).is_err());
        assert!(parse_memory("VM (page size of 0 bytes)\nPages free: 1", 1024).is_err());
        assert!(parse_swap("total = NaNM free = 1M").is_err());
        assert!(parse_swap("total = 1M free = 2M").is_err());
        assert!(parse_swap("total = 1雪 free = 1M").is_err());
    }
}
