//! proc(5): comm may contain spaces/parentheses; starttime distinguishes reused PIDs.
use super::{ProcessCpu, ProcessSample, ProcessesSample};
pub(crate) const SCRIPT: &str = r#"
printf '\n[processes]\n'
printf 'PAGE\t'; getconf PAGESIZE 2>/dev/null || printf '0\n'
count=0
for directory in /proc/[0-9]*; do
    [ -d "$directory" ] || continue
    count=$((count + 1))
    [ "$count" -le 8192 ] || continue
    if IFS= read -r value 2>/dev/null < "$directory/stat"; then
        printf 'P\t%s\n' "$value"
    fi
done
printf 'COUNT\t%s\n' "$count"
"#;
pub(in crate::resources) fn parse(contents: &str, total_cpu: u64) -> Option<ProcessesSample> {
    let page_size = contents
        .lines()
        .find_map(|line| line.strip_prefix("PAGE\t")?.parse::<u64>().ok())
        .filter(|value| *value > 0);
    let total_count = contents
        .lines()
        .find_map(|line| line.strip_prefix("COUNT\t")?.parse::<usize>().ok())?;
    let entries = contents
        .lines()
        .filter_map(|line| parse_process(line.strip_prefix("P\t")?, page_size))
        .take(8192)
        .collect();
    Some(ProcessesSample {
        total_cpu: Some(total_cpu),
        entries,
        total_count,
        truncated: total_count > 8192,
    })
}
fn parse_process(line: &str, page_size: Option<u64>) -> Option<ProcessSample> {
    let (pid, remainder) = line.split_once(" (")?;
    let (name, tail) = remainder.rsplit_once(") ")?;
    if name.chars().any(char::is_control) {
        return None;
    }
    let fields: Vec<_> = tail.split_whitespace().collect();
    // The tail begins with field 3 (state). CPU: fields14/15, start:22, RSS:24.
    let cpu = fields
        .get(11)?
        .parse::<u64>()
        .ok()?
        .checked_add(fields.get(12)?.parse().ok()?)?;
    let start_id = fields.get(19)?.parse::<u64>().ok()?.to_string();
    let memory_bytes = fields
        .get(21)
        .and_then(|rss| rss.parse::<u64>().ok())
        .zip(page_size)
        .and_then(|(rss, page)| rss.checked_mul(page));
    Some(ProcessSample {
        pid: pid.parse().ok()?,
        start_id,
        name: name.chars().take(256).collect(),
        user: None,
        memory_bytes,
        cpu: ProcessCpu::Counter(cpu),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_name_parentheses_start_identity_and_page_units() -> anyhow::Result<()> {
        let row = "17 (name ) with spaces) S 1 2 3 4 5 6 7 8 9 10 20 30 0 0 0 0 0 0 123 0 5";
        let sample = parse_process(row, Some(65536)).ok_or_else(|| anyhow::anyhow!("process"))?;
        assert_eq!(sample.name, "name ) with spaces");
        assert_eq!(sample.start_id, "123");
        assert_eq!(sample.memory_bytes, Some(5 * 65536));
        assert!(matches!(sample.cpu, ProcessCpu::Counter(50)));
        assert!(parse_process("17 (malformed) S 0", Some(4096)).is_none());
        Ok(())
    }
}
