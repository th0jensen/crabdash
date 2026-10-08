//! Non-loopback link counters; address aliases never count an interface twice.
use super::super::NetworkCounter;
use super::super::collector::ResourceCollector;
use anyhow::{Context as _, Result, ensure};
use std::collections::HashSet;
use utils::{args, args::Args};

pub(crate) async fn sample(machine: &mut ResourceCollector<'_>) -> Result<Vec<NetworkCounter>> {
    let output = machine
        .run("sh", &args!["-c", "LC_ALL=C /usr/sbin/netstat -ibn"])
        .await?;
    parse(&output)
}

fn parse(output: &str) -> Result<Vec<NetworkCounter>> {
    let mut lines = output.lines();
    let header = lines
        .next()
        .context("Missing macOS interface header")?
        .split_whitespace()
        .collect::<Vec<_>>();
    let first_stat = header
        .iter()
        .position(|field| *field == "Ipkts")
        .context("Missing macOS interface counters")?;
    let received = header
        .iter()
        .position(|field| *field == "Ibytes")
        .context("Missing macOS received bytes")?
        .checked_sub(first_stat)
        .context("Invalid interface header")?;
    let sent = header
        .iter()
        .position(|field| *field == "Obytes")
        .context("Missing macOS sent bytes")?
        .checked_sub(first_stat)
        .context("Invalid interface header")?;
    let stat_count = header.len() - first_stat;
    let mut ids = HashSet::new();
    let mut counters = Vec::new();
    for line in lines {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if !fields.iter().any(|field| field.starts_with("<Link#")) {
            continue;
        }
        ensure!(
            fields.len() >= stat_count + 3,
            "Incomplete macOS link counters"
        );
        let id = fields[0].trim_end_matches('*').to_owned();
        if id == "lo0" || !ids.insert(id.clone()) {
            continue;
        }
        // Locate statistics from the right: virtual interfaces may lack a
        // hardware Address token, so not every row has four leading fields.
        let stats = &fields[fields.len() - stat_count..];
        counters.push(NetworkCounter {
            id,
            received_bytes: stats[received]
                .parse()
                .context("Invalid interface received bytes")?,
            sent_bytes: stats[sent]
                .parse()
                .context("Invalid interface sent bytes")?,
        });
    }
    Ok(counters)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loopback_exclusion_retains_aliased_physical_and_virtual_interfaces() -> Result<()> {
        let output = "Name Mtu Network Address Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll\nlo0* 16384 <Link#1> 10 0 1000 20 0 2000 0\nlo0 16384 127 127.0.0.1 10 - 1000 20 - 2000 -\nen0* 1500 <Link#2> aa:bb 30 0 3000 40 0 4000 0\nen0 1500 <Link#2> aa:bb 30 0 3000 40 0 4000 0\nutun4 1380 <Link#3> 50 0 5000 60 0 6000 0\nbridge0 1500 <Link#4> cc:dd 70 0 7000 80 0 8000 0\n";
        let counters = parse(output)?;
        assert_eq!(
            counters
                .iter()
                .map(|counter| (
                    counter.id.as_str(),
                    counter.received_bytes,
                    counter.sent_bytes
                ))
                .collect::<Vec<_>>(),
            [
                ("en0", 3000, 4000),
                ("utun4", 5000, 6000),
                ("bridge0", 7000, 8000)
            ]
        );
        assert!(parse("Name Mtu Network Address Ipkts Ierrs\n").is_err());
        Ok(())
    }

    #[test]
    fn rising_loopback_traffic_does_not_change_idle_external_rates() -> Result<()> {
        use crate::resources::{CpuSample, MemorySample, ResourceMonitor, ResourceSample};
        use std::time::{Duration, Instant};
        let snapshot = |loopback| {
            format!(
                "Name Mtu Network Address Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll\nlo0* 16384 <Link#1> 10 0 {loopback} 20 0 {loopback} 0\nen0* 1500 <Link#2> aa:bb 30 0 3000 40 0 4000 0\nen0 1500 <Link#2> aa:bb 30 0 3000 40 0 4000 0\n"
            )
        };
        let start = Instant::now();
        let sample = |network, captured_at, uptime_seconds| ResourceSample {
            cpu: CpuSample::Sampled { percent: 0.0 },
            logical_cpus: 1,
            memory: MemorySample {
                total_bytes: 4096,
                available_bytes: 4096,
                estimated: false,
            },
            swap: None,
            load_average: None,
            uptime_seconds,
            boot_id: "fixture".into(),
            captured_at,
            processes: None,
            network: Some(network),
            disks: None,
            gpus: None,
        };
        let mut monitor = ResourceMonitor::default();
        monitor.update(sample(parse(&snapshot(1000))?, start, 0.0))?;
        let usage = monitor.update(sample(
            parse(&snapshot(1_000_000))?,
            start + Duration::from_secs(2),
            2.0,
        ))?;
        let rows = usage.network.context("Expected network counters")?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "en0");
        assert_eq!(rows[0].received_bytes_per_second, Some(0.0));
        assert_eq!(rows[0].sent_bytes_per_second, Some(0.0));
        Ok(())
    }

    #[test]
    fn loopback_only_is_a_known_empty_inventory() -> Result<()> {
        let output = "Name Mtu Network Address Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll\nlo0* 16384 <Link#1> 10 0 1000 20 0 2000 0\nlo0 16384 127 127.0.0.1 10 - 1000 20 - 2000 -\n";
        assert!(parse(output)?.is_empty());
        Ok(())
    }
}
