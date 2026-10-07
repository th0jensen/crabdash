use super::NetworkCounter;
pub(crate) const SCRIPT: &str = "printf '\\n[network]\\n'; cat /proc/net/dev 2>/dev/null || true\n";
pub(in crate::resources) fn parse(contents: &str) -> Option<Vec<NetworkCounter>> {
    if !contents.contains("bytes") {
        return None;
    }
    let mut counters: Vec<_> = contents
        .lines()
        .filter_map(|line| {
            let (id, values) = line.split_once(':')?;
            let id = id.trim();
            if id.is_empty() || id == "lo" {
                return None;
            }
            let values: Vec<_> = values.split_whitespace().collect();
            Some(NetworkCounter {
                id: id.to_owned(),
                received_bytes: values.first()?.parse().ok()?,
                sent_bytes: values.get(8)?.parse().ok()?,
            })
        })
        .collect();
    counters.sort_by(|a, b| a.id.cmp(&b.id));
    counters.dedup_by(|a, b| a.id == b.id);
    Some(counters)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_byte_columns_and_excludes_loopback() -> anyhow::Result<()> {
        let values = parse("Inter-| Receive bytes | Transmit bytes\n lo: 5 0 0 0 0 0 0 0 8 0\n eth0: 100 1 0 0 0 0 0 0 300 1 0 0 0 0 0 0").ok_or_else(|| anyhow::anyhow!("network"))?;
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].received_bytes, 100);
        assert_eq!(values[0].sent_bytes, 300);
        Ok(())
    }
}
