//! proc(5): comm may contain spaces/parentheses; starttime distinguishes reused PIDs.
use super::{ProcessCpu, ProcessSample, ProcessesSample};
use std::collections::HashMap;
pub(crate) const SCRIPT: &str = r#"
printf '\n[processes]\n'
printf 'PAGE\t'; getconf PAGESIZE 2>/dev/null || printf '0\n'
# One bounded local UID map; never query a directory service per process.
head -c 1048576 /etc/passwd 2>/dev/null | while IFS=: read -r user password uid rest; do
    case "$uid" in ''|*[!0-9]*) continue;; esac
    printf 'U\t%s\t%s\n' "$uid" "$user"
done || true
count=0
for directory in /proc/[0-9]*; do
    [ -d "$directory" ] || continue
    count=$((count + 1))
    [ "$count" -le 8192 ] || continue
    if IFS= read -r value 2>/dev/null < "$directory/stat"; then
        printf 'P\t%s\n' "$value"
        uid=''
        while IFS= read -r status; do
            case "$status" in
                Uid:*) set -- $status; uid=${3:-}; break;;
            esac
        done 2>/dev/null < "$directory/status" || true
        # Only attach the status owner if this is still the same process.
        if [ -n "$uid" ] && IFS= read -r after 2>/dev/null < "$directory/stat"; then
            printf 'O\t%s\t%s\n' "$uid" "$after"
        fi
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
    let mut accounts = HashMap::new();
    for line in contents
        .lines()
        .filter_map(|line| line.strip_prefix("U\t"))
        .take(8192)
    {
        let Some((uid, name)) = line.split_once('\t') else {
            continue;
        };
        let Ok(uid) = uid.parse::<u32>() else {
            continue;
        };
        if !name.is_empty() && name.len() <= 4096 && !name.chars().any(char::is_control) {
            accounts.entry(uid).or_insert_with(|| name.to_string());
        }
    }
    let owners: HashMap<_, _> = contents
        .lines()
        .filter_map(|line| {
            let (uid, after) = line.strip_prefix("O\t")?.split_once('\t')?;
            let uid = uid.parse::<u32>().ok()?;
            let after = parse_process(after, None)?;
            Some(((after.pid, after.start_id), uid))
        })
        .take(8192)
        .collect();
    let entries = contents
        .lines()
        .filter_map(|line| parse_process(line.strip_prefix("P\t")?, page_size))
        .take(8192)
        .map(|mut process| {
            process.user = owners
                .get(&(process.pid, process.start_id.clone()))
                .map(|uid| {
                    accounts
                        .get(uid)
                        .cloned()
                        .unwrap_or_else(|| format!("UID {uid}"))
                });
            process
        })
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

    #[test]
    fn effective_uid_names_numeric_fallback_and_reused_pid_keep_counter_identity()
    -> anyhow::Result<()> {
        let stat = |pid, start| {
            format!("{pid} (worker 雪) S 1 2 3 4 5 6 7 8 9 10 20 30 0 0 0 0 0 0 {start} 0 5")
        };
        let output = format!(
            "PAGE\t4096\nU\t1001\t雪-owner\nU\t1002\t\nP\t{}\nO\t1001\t{}\nP\t{}\nO\t1002\t{}\nP\t{}\nP\t{}\nO\t1001\t{}\nCOUNT\t4\n",
            stat(1, 100),
            stat(1, 100),
            stat(2, 200),
            stat(2, 200),
            stat(3, 300),
            stat(4, 400),
            stat(4, 401)
        );
        let sample = parse(&output, 500).ok_or_else(|| anyhow::anyhow!("Process snapshot"))?;
        assert_eq!(sample.entries.len(), 4);
        assert_eq!(sample.entries[0].user.as_deref(), Some("雪-owner"));
        assert_eq!(sample.entries[1].user.as_deref(), Some("UID 1002"));
        assert_eq!(sample.entries[2].user, None); // absent/inaccessible status
        assert_eq!(sample.entries[3].user, None); // PID reused during status read
        assert_eq!(sample.entries[3].start_id, "400");
        assert!(matches!(sample.entries[3].cpu, ProcessCpu::Counter(50)));
        assert_eq!(sample.total_cpu, Some(500));
        Ok(())
    }
}
