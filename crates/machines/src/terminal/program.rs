//! Foreground process names for terminal chrome. Discovery never runs on the UI thread.
#[cfg(not(target_os = "windows"))]
mod unix;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(not(target_os = "windows"))]
pub(super) use unix::foreground;
#[cfg(target_os = "windows")]
pub(super) use windows::foreground;

pub(super) fn name(command: &str) -> Option<String> {
    let command = command.trim().trim_start_matches('-');
    let command = command
        .rsplit(['/', '\\'])
        .next()?
        .trim()
        .trim_start_matches('-');
    let command = command.strip_suffix(".exe").unwrap_or(command);
    let name: String = command
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    (!name.is_empty()).then_some(name)
}

/// A Node CLI launcher is its user-facing program, rather than "node".
#[cfg(any(not(target_os = "windows"), test))]
fn launcher_name(program: String, arguments: &str) -> String {
    if !matches!(program.as_str(), "node" | "nodejs") {
        return program;
    }
    let script = arguments
        .split(['\0', ' '])
        .skip(1)
        .find(|arg| !arg.is_empty() && !arg.starts_with('-'));
    let script = script
        .and_then(name)
        .map(|name| name.trim_end_matches(".js").to_owned());
    match script.as_deref() {
        Some("codex" | "claude") => script.unwrap_or(program),
        _ => program,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn node_cli_launchers_use_the_program_name_without_renaming_arbitrary_scripts() {
        assert_eq!(
            super::launcher_name("node".into(), "/usr/bin/node\0/opt/codex.js\0"),
            "codex"
        );
        assert_eq!(
            super::launcher_name("node".into(), "/usr/bin/node /opt/unrelated.js"),
            "node"
        );
    }

    #[test]
    fn process_names_do_not_expose_paths_or_login_shell_markers() {
        assert_eq!(super::name("/bin/-bash\n"), Some("bash".into()));
        assert_eq!(super::name("-zsh"), Some("zsh".into()));
        assert_eq!(
            super::name("C:\\Program Files\\PowerShell\\pwsh.exe"),
            Some("pwsh".into())
        );
        assert_eq!(super::name("/opt/tools/雪"), Some("雪".into()));
        assert_eq!(super::name(""), None);
    }
}
