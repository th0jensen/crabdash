//! The PTY supplies its foreground process group, so pipelines and nested shells
//! follow the actual terminal job rather than the login shell's oldest child.
pub(in crate::terminal) fn foreground(pid: Option<i32>) -> Option<String> {
    let pid = pid.filter(|pid| *pid > 0)?;
    #[cfg(target_os = "linux")]
    let command = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    #[cfg(not(target_os = "linux"))]
    let command = {
        let output = std::process::Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout).ok()?
    };
    let program = super::name(&command)?;
    if !matches!(program.as_str(), "node" | "nodejs") {
        return Some(program);
    }
    #[cfg(target_os = "linux")]
    let arguments = std::fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    #[cfg(not(target_os = "linux"))]
    let arguments = std::process::Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "args="])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok());
    Some(
        arguments
            .map(|args| super::launcher_name(program.clone(), &args))
            .unwrap_or(program),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_process_lookup_identifies_the_executable_and_rejects_invalid_pids()
    -> std::io::Result<()> {
        let mut child = std::process::Command::new("/bin/sleep").arg("3").spawn()?;
        let result = super::foreground(i32::try_from(child.id()).ok());
        // Reap even if discovery failed; this regression must not leave a job.
        let _ = child.kill();
        child.wait()?;
        assert_eq!(result.as_deref(), Some("sleep"));
        assert_eq!(super::foreground(Some(-1)), None);
        assert_eq!(super::foreground(None), None);
        Ok(())
    }
}
