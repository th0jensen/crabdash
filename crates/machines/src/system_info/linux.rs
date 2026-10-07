//! Linux identity discovery, through the same local/SSH transport as commands.
use super::LinuxDistribution;
use crate::machine::Machine;
use anyhow::Result;
use utils::{args, args::Args};

pub(super) async fn distribution(machine: &mut Machine) -> Result<Option<LinuxDistribution>> {
    // /etc takes precedence; never source os-release or evaluate its contents.
    let output = machine.run("sh", &args!["-c",
        "if [ -e /etc/os-release ]; then cat /etc/os-release; else cat /usr/lib/os-release; fi"
    ]).await?;
    Ok(parse_os_release(&String::from(output)))
}

fn parse_os_release(contents: &str) -> Option<LinuxDistribution> {
    let mut id = None;
    let mut name = None;
    let mut pretty_name = None;
    for line in contents.lines().map(str::trim) {
        if line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let Some(value) = unquote(raw.trim()) else {
            continue;
        };
        match key.trim() {
            "ID" => id = Some(value),
            "NAME" => name = Some(value),
            "PRETTY_NAME" => pretty_name = Some(value),
            _ => {}
        }
    }
    if id.is_none() && name.is_none() && pretty_name.is_none() {
        return None;
    }
    let id = id
        .filter(|id| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
        })
        .unwrap_or_else(|| "linux".into());
    let name = name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Linux".into());
    let pretty_name = pretty_name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| name.clone());
    Some(LinuxDistribution {
        id,
        name,
        pretty_name,
    })
}

fn unquote(raw: &str) -> Option<String> {
    if raw.chars().any(char::is_control) {
        return None;
    }
    if raw.starts_with('\'') {
        return raw
            .strip_prefix('\'')?
            .strip_suffix('\'')
            .map(str::to_owned);
    }
    let raw = if raw.starts_with('"') {
        raw.strip_prefix('"')?.strip_suffix('"')?
    } else {
        raw
    };
    let mut value = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\'
            && chars
                .peek()
                .is_some_and(|next| matches!(next, '$' | '`' | '"' | '\\'))
        {
            value.push(chars.next()?);
        } else {
            value.push(ch);
        }
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_vendor_identity_without_guessing_from_kernel_or_hostname() {
        let distro = parse_os_release(
            "# distro\nID=cachyos\nID_LIKE=arch\nNAME='CachyOS'\nPRETTY_NAME=\"CachyOS Linux\"\n",
        )
        .unwrap();
        assert_eq!(distro.id, "cachyos");
        assert_eq!(distro.name, "CachyOS");
        assert_eq!(distro.pretty_name, "CachyOS Linux");
        assert!(parse_os_release("# unavailable\nINVALID").is_none());
    }

    #[test]
    fn quoted_values_are_data_and_last_assignment_wins() {
        let distro = parse_os_release(
            r#"ID=debian
ID="fedora"
NAME="Fedora"
PRETTY_NAME="Fedora \"Workstation\" \$(not-a-command)"
"#,
        )
        .unwrap();
        assert_eq!(distro.id, "fedora");
        assert_eq!(
            distro.pretty_name,
            "Fedora \"Workstation\" $(not-a-command)"
        );
        assert_eq!(unquote("'literal\\$value'"), Some("literal\\$value".into()));
        assert!(unquote("\"unfinished").is_none());
    }
}
