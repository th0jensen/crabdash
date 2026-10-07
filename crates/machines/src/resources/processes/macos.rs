//! BSD ps exposes cumulative CPU time and a stable start timestamp; %cpu does not.
use super::super::collector::ResourceCollector;
use super::super::{ProcessCpu, ProcessSample, ProcessesSample};
use anyhow::{Context as _, Result, ensure};
use utils::{args, args::Args};

const LIMIT: usize = 8192;
pub(crate) async fn sample(machine: &mut ResourceCollector<'_>) -> Result<ProcessesSample> {
    let output = machine
        .run(
            "sh",
            &args!["-c", "LC_ALL=C /bin/ps -axo pid=,lstart=,time=,rss=,comm="],
        )
        .await?;
    parse(&output)
}

fn parse(output: &str) -> Result<ProcessesSample> {
    let mut entries = Vec::new();
    let mut total_count = 0;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        total_count += 1;
        if entries.len() >= LIMIT {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        ensure!(fields.len() >= 9, "Incomplete macOS process row");
        let pid = fields[0].parse().context("Invalid macOS process PID")?;
        let cpu = cpu_centiseconds(fields[6])?;
        let memory_bytes = fields[7]
            .parse::<u64>()
            .context("Invalid process resident memory")?
            .checked_mul(1024)
            .context("Process resident memory overflow")?;
        entries.push(ProcessSample {
            pid,
            start_id: fields[1..6].join(" "),
            name: fields[8..].join(" "),
            user: None,
            memory_bytes: Some(memory_bytes),
            cpu: ProcessCpu::TimedCounter {
                ticks: cpu,
                ticks_per_second: 100,
            },
        });
    }
    Ok(ProcessesSample {
        total_cpu: None,
        entries,
        total_count,
        truncated: total_count > LIMIT,
    })
}

fn cpu_centiseconds(value: &str) -> Result<u64> {
    let (days, value) = match value.split_once('-') {
        Some((days, value)) => (
            days.parse::<u64>().context("Invalid process CPU days")?,
            value,
        ),
        None => (0, value),
    };
    let values = value.split(':').collect::<Vec<_>>();
    ensure!(
        (2..=3).contains(&values.len()),
        "Invalid process CPU duration"
    );
    let last = values.last().context("Missing process CPU seconds")?;
    let (seconds, fraction) = last.split_once('.').map_or((*last, ""), |parts| parts);
    let seconds = seconds
        .parse::<u64>()
        .context("Invalid process CPU seconds")?;
    ensure!(
        seconds < 60 && fraction.len() <= 2 && fraction.bytes().all(|value| value.is_ascii_digit()),
        "Invalid process CPU fraction"
    );
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>()? * if fraction.len() == 1 { 10 } else { 1 }
    };
    let minutes = values[values.len() - 2]
        .parse::<u64>()
        .context("Invalid process CPU minutes")?;
    let hours = if values.len() == 3 {
        values[0]
            .parse::<u64>()
            .context("Invalid process CPU hours")?
    } else {
        0
    };
    ensure!(
        values.len() == 2 || minutes < 60,
        "Invalid process CPU minutes"
    );
    days.checked_mul(24)
        .and_then(|value| value.checked_add(hours))
        .and_then(|value| value.checked_mul(60))
        .and_then(|value| value.checked_add(minutes))
        .and_then(|value| value.checked_mul(60))
        .and_then(|value| value.checked_add(seconds))
        .and_then(|value| value.checked_mul(100))
        .and_then(|value| value.checked_add(fraction))
        .context("Process CPU duration overflow")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cumulative_time_and_pid_start_identity_are_preserved() -> Result<()> {
        let processes = parse(" 12 Sat Oct 7 12:00:00 2026 1:02.34 2048 /Applications/My App\n")?;
        assert_eq!(processes.total_cpu, None);
        assert_eq!(processes.total_count, 1);
        assert!(!processes.truncated);
        assert_eq!(processes.entries[0].start_id, "Sat Oct 7 12:00:00 2026");
        assert_eq!(processes.entries[0].name, "/Applications/My App");
        assert_eq!(processes.entries[0].memory_bytes, Some(2048 * 1024));
        assert!(matches!(
            processes.entries[0].cpu,
            ProcessCpu::TimedCounter {
                ticks: 6234,
                ticks_per_second: 100
            }
        ));
        assert_eq!(cpu_centiseconds("2-01:02:03.4")?, 17652340);
        assert!(cpu_centiseconds("1:60.00").is_err());
        assert!(cpu_centiseconds("1:02.雪").is_err());
        Ok(())
    }
}
