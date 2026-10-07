//! Link-layer byte counters: address aliases must not count an interface twice.
use super::super::NetworkCounter;
use crate::machine::Machine;
use anyhow::{Context as _, Result, ensure};
use std::collections::HashSet;
use utils::{args, args::Args};

pub(crate) async fn sample(machine: &mut Machine) -> Result<Vec<NetworkCounter>> {
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
        if !ids.insert(id.clone()) {
            continue;
        }
        // Loopback lacks a hardware Address token. Locate numeric statistics
        // from the right instead of assuming every row has four leading fields.
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
    fn aliases_and_missing_loopback_address_do_not_duplicate_bytes() -> Result<()> {
        let output = "Name Mtu Network Address Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll\nlo0 16384 <Link#1> 10 0 1000 20 0 2000 0\nlo0 16384 127 127.0.0.1 10 - 1000 20 - 2000 -\nen0* 1500 <Link#2> aa:bb 30 0 3000 40 0 4000 0\nen0 1500 <Link#2> aa:bb 30 0 3000 40 0 4000 0\n";
        let counters = parse(output)?;
        assert_eq!(counters.len(), 2);
        assert_eq!(counters[0].received_bytes, 1000);
        assert_eq!(counters[1].id, "en0");
        assert_eq!(counters[1].sent_bytes, 4000);
        assert!(parse("Name Mtu Network Address Ipkts Ierrs\n").is_err());
        Ok(())
    }
}
