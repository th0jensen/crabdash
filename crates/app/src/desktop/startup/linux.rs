use anyhow::{Context as _, Result, bail};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
fn config_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|path| Path::new(path).is_absolute())
    {
        return Ok(PathBuf::from(path));
    }
    env::var_os("HOME")
        .map(|path| PathBuf::from(path).join(".config"))
        .context("Unable to find your desktop configuration directory")
}

pub(super) fn startup_enabled() -> Result<bool> {
    let output = Command::new("systemctl")
        .args(["--user", "is-enabled", "crabdash.service"])
        .output()
        .context("systemd user services are unavailable")?;
    let state = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        return Ok(matches!(state.trim(), "enabled" | "enabled-runtime"));
    }
    // A missing or disabled unit is the normal initial state.
    let error = String::from_utf8_lossy(&output.stderr);
    if state.trim() == "disabled"
        || error.contains("does not exist")
        || error.contains("No such file")
        || state.trim() == "not-found"
    {
        return Ok(false);
    }
    bail!("Unable to read login startup: {}", error.trim())
}

fn systemctl(args: &[&str]) -> Result<()> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .context("Unable to contact your systemd user session")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(())
}

fn service_unit(executable: &Path) -> Result<String> {
    let path = executable
        .to_str()
        .context("The application path is not valid UTF-8")?;
    if path.contains(['\n', '\r']) {
        bail!("The application path contains a line break");
    }
    let escaped = path
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    Ok(format!(
        "[Unit]\nDescription=Crabdash desktop dashboard\nPartOf=graphical-session.target\nAfter=graphical-session.target\n\n[Service]\nType=exec\nExecStart=\"{escaped}\"\nRestart=no\n\n[Install]\nWantedBy=graphical-session.target\n"
    ))
}

/// Only affects the next graphical login; toggling never starts a second app.
pub(super) fn set_login_startup(enabled: bool) -> Result<()> {
    if !enabled {
        return systemctl(&["disable", "crabdash.service"]);
    }
    let directory = config_dir()?.join("systemd/user");
    fs::create_dir_all(&directory).context("Unable to create the user service directory")?;
    let unit = directory.join("crabdash.service");
    let previous = match fs::read(&unit) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("Unable to read the existing user service"),
    };
    let contents = service_unit(&env::current_exe().context("Unable to locate Crabdash")?)?;
    fs::write(&unit, contents).context("Unable to write the Crabdash user service")?;
    if let Err(error) =
        systemctl(&["daemon-reload"]).and_then(|_| systemctl(&["enable", "crabdash.service"]))
    {
        if let Some(previous) = previous {
            fs::write(&unit, previous).ok();
        } else {
            fs::remove_file(&unit).ok();
        }
        systemctl(&["daemon-reload"]).ok();
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_quotes_paths_and_stops_with_the_graphical_session() -> Result<()> {
        let unit = service_unit(Path::new("/home/test/My Apps/Crab\"dash%$test/crabdash"))?;
        assert!(unit.contains("ExecStart=\"/home/test/My Apps/Crab\\\"dash%%$$test/crabdash\""));
        assert!(unit.contains("PartOf=graphical-session.target"));
        assert!(unit.contains("WantedBy=graphical-session.target"));
        assert!(unit.contains("Restart=no"));
        Ok(())
    }

    #[test]
    fn rejects_paths_that_could_inject_unit_directives() {
        assert!(service_unit(Path::new("/tmp/crabdash\nRestart=always")).is_err());
    }
}

pub(super) const SUPPORTED: bool = true;

pub(super) fn startup_warning() -> Result<Option<String>> {
    Ok(None)
}
