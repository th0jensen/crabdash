//! A tab identifies the running program. Explicit session names still win.
//! SSH cannot inspect a PTY process group: command-bearing OSC titles are used
//! when supplied by the remote shell/application, otherwise the shell fallback.
pub(super) fn automatic(program: Option<&str>, osc: &str, local: bool) -> String {
    if let Some(program) = program.and_then(command_name) {
        return program;
    }
    if let Some(program) = osc_program(osc) {
        return program;
    }
    if local {
        if cfg!(target_os = "windows") {
            return "powershell".into();
        }
        if let Ok(shell) = std::env::var("SHELL")
            && let Some(shell) = command_name(&shell)
        {
            return shell;
        }
    }
    "sh".into()
}

fn command_name(value: &str) -> Option<String> {
    let name = value
        .trim()
        .rsplit(['/', '\\'])
        .next()?
        .trim_start_matches('-');
    let name = name.strip_suffix(".exe").unwrap_or(name);
    (!name.is_empty()).then(|| name.chars().filter(|c| !c.is_control()).take(64).collect())
}

fn osc_program(osc: &str) -> Option<String> {
    let osc = osc.trim();
    // Default shell prompt titles describe user@host and cwd, not a program.
    if osc.is_empty() || osc.contains('@') || osc.starts_with(['~', '/']) {
        return None;
    }
    if osc.contains('\\') {
        return osc.ends_with(".exe").then(|| command_name(osc)).flatten();
    }
    let name = osc.split(|c: char| c.is_whitespace() || c == ':').next()?;
    if name.is_empty()
        || name.contains('/')
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || "-_.".contains(c))
    {
        return None;
    }
    command_name(name)
}

#[cfg(test)]
mod tests {
    use super::automatic;
    #[test]
    fn foreground_program_wins_over_shell_cwd_titles() {
        assert_eq!(automatic(Some("codex"), "user@host:~/src", false), "codex");
        assert_eq!(automatic(Some("/bin/-bash"), "~/src", false), "bash");
    }
    #[test]
    fn remote_application_titles_identify_the_program_without_a_cwd_title() {
        assert_eq!(automatic(None, "vim: src/main.rs", false), "vim");
        assert_eq!(automatic(None, "codex — crabdash", false), "codex");
        assert_eq!(automatic(None, "user@fedora:~/src", false), "sh");
        assert_eq!(automatic(None, "/home/user/src", false), "sh");
        assert_eq!(automatic(None, "~", false), "sh");
        assert_eq!(automatic(None, "", false), "sh");
        assert_eq!(
            automatic(None, "C:\\Windows\\powershell.exe", false),
            "powershell"
        );
    }
}
