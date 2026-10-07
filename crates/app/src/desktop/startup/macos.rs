//! Native login items for application bundles; LaunchAgents for developer binaries.
use anyhow::{Context as _, Result, bail};
use objc2::{msg_send, rc::Retained, runtime::AnyClass, runtime::AnyObject};
use objc2_foundation::NSError;
use std::{env, fs, path::PathBuf, process::Command};

// Load the public ServiceManagement framework before looking up its runtime class.
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

pub(super) const SUPPORTED: bool = true;
const LABEL: &str = "com.crabdash.login";

fn bundled_service() -> Result<Option<Retained<AnyObject>>> {
    let executable = env::current_exe().context("Unable to locate Crabdash")?;
    if !executable
        .ancestors()
        .any(|path| path.extension().is_some_and(|ext| ext == "app"))
    {
        return Ok(None);
    }
    let Some(class) = AnyClass::get(c"SMAppService") else {
        return Ok(None);
    };
    // SAFETY: SMAppService is a public class available since macOS 13; this
    // selector returns the retained main application's login service.
    Ok(Some(unsafe { msg_send![class, mainAppService] }))
}

fn agent_path() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("Unable to locate your home directory")?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

pub(super) fn startup_enabled() -> Result<bool> {
    if let Some(service) = bundled_service()? {
        // SAFETY: Public SMAppServiceStatus getter; values are stable NSInteger.
        let status: isize = unsafe { msg_send![&*service, status] };
        return match status {
            1 | 2 => Ok(true),
            0 | 3 => Ok(false),
            _ => bail!("macOS reported an unknown login item status ({status})"),
        };
    }
    Ok(agent_path()?
        .try_exists()
        .context("Unable to inspect your login agent")?)
}

/// Registration remains enabled while macOS is waiting for user approval; the
/// warning explains why the app will not launch yet and the switch can disable it.
pub(super) fn startup_warning() -> Result<Option<String>> {
    if let Some(service) = bundled_service()? {
        let status: isize = unsafe { msg_send![&*service, status] };
        if status == 2 {
            return Ok(Some("Allow Crabdash in System Settings > General > Login Items to complete login startup".into()));
        }
    }
    Ok(None)
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn enable_agent() -> Result<()> {
    let uid = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .context("Unable to identify your login session")?;
    if !uid.status.success() {
        bail!("Unable to identify your login session");
    }
    let uid = String::from_utf8(uid.stdout).context("Invalid login session identifier")?;
    let output = Command::new("/bin/launchctl")
        .args(["enable", &format!("gui/{}/{LABEL}", uid.trim())])
        .output()
        .context("Unable to contact your macOS login session")?;
    if !output.status.success() {
        bail!(
            "Unable to enable login startup: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub(super) fn set_login_startup(enabled: bool) -> Result<()> {
    if let Some(service) = bundled_service()? {
        let mut error: Option<Retained<NSError>> = None;
        // SAFETY: These public ServiceManagement selectors receive an NSError
        // out-parameter whose lifetime is managed by objc2.
        let success: bool = unsafe {
            if enabled {
                msg_send![&*service, registerAndReturnError: &mut error]
            } else {
                msg_send![&*service, unregisterAndReturnError: &mut error]
            }
        };
        if !success {
            bail!(
                "{}",
                error
                    .map(|error| error.localizedDescription().to_string())
                    .unwrap_or_else(|| "macOS could not change the login item".into())
            );
        }
        // Remove a developer LaunchAgent when migrating to a bundled app so
        // macOS has only one owner for future login launches.
        let fallback = agent_path()?;
        match fs::remove_file(fallback) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).context("Unable to remove the previous developer login agent");
            }
        }
        return Ok(());
    }
    let path = agent_path()?;
    if !enabled {
        match fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).context("Unable to remove the Crabdash login agent"),
        }
    }
    let executable = env::current_exe().context("Unable to locate Crabdash")?;
    let executable = executable
        .to_str()
        .context("Crabdash's path is not valid UTF-8")?;
    let parent = path.parent().context("Invalid login agent path")?;
    fs::create_dir_all(parent).context("Unable to create your LaunchAgents directory")?;
    let previous = match fs::read(&path) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("Unable to read your existing login agent"),
    };
    let contents = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{LABEL}</string>\n<key>ProgramArguments</key><array><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n<key>LimitLoadToSessionType</key><string>Aqua</string>\n<key>ProcessType</key><string>Interactive</string>\n</dict></plist>\n",
        xml(executable)
    );
    let temporary = path.with_extension(format!("plist.{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temporary, contents).context("Unable to save your login agent")?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(temporary);
        return Err(error).context("Unable to install your login agent");
    }
    // Do not bootstrap: RunAtLoad would spawn another copy now. launchd loads
    // ~/Library/LaunchAgents on the next graphical login.
    if let Err(error) = enable_agent() {
        match previous {
            Some(contents) => {
                let _ = fs::write(&path, contents);
            }
            None => {
                let _ = fs::remove_file(&path);
            }
        }
        return Err(error);
    }
    Ok(())
}
