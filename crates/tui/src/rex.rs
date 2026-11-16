//! Installs crabdash-tui as a Rex plugin: a Lua file beside Rex's config and a
//! marked block in init.lua that loads it. Rex then offers its actions in the
//! command palette of every app connected to this machine.
use anyhow::{Context, Result, anyhow, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const BEGIN: &str = "-- >>> crabdash >>>";
const END: &str = "-- <<< crabdash <<<";
const PLUGIN: &str = "-- plugin: ";
const SOURCE: &str = include_str!("../lua/crabdash.lua");

pub fn install() -> Result<String> {
    let bin = this_binary()?;
    let config = config_path()?;
    let plugin = plugin_path(&config);
    let previous = fs::read_to_string(&config).unwrap_or_default();
    write_if_changed(&plugin, SOURCE)?;
    write_if_changed(
        &config,
        &with_block(&previous, Some(&loader(&plugin, &bin))),
    )?;
    if let Err(error) = rex(&["config", "check", &config.display().to_string()]) {
        fs::write(&config, previous).context("Could not restore Rex's config")?;
        return Err(error);
    }
    let reloaded = rex(&["config", "reload"]).is_ok();
    Ok(format!(
        "Crabdash is in Rex's command palette{}. Open it and type Crabdash:\n  \
         Crabdash: Open Dashboard opens a tab with a pane for each section.\n  \
         Crabdash: Docker, Disks, Services and System open one beside the current pane.\n\
         Every Rex app connected to this machine offers them, including Rex for iOS.",
        if reloaded {
            ""
        } else {
            " after Rex reloads its config (run rex config reload)"
        }
    ))
}

pub fn uninstall() -> Result<String> {
    let config = config_path()?;
    let previous = fs::read_to_string(&config).unwrap_or_default();
    let next = with_block(&previous, None);
    if next != previous {
        fs::write(&config, next)
            .with_context(|| format!("Could not write {}", config.display()))?;
        let _ = rex(&["config", "reload"]);
    }
    let plugin = plugin_path(&config);
    if plugin.is_file() {
        fs::remove_file(&plugin)?;
    }
    if let Some(folder) = plugin.parent() {
        // Only when empty: something else may have been put there.
        let _ = fs::remove_dir(folder);
    }
    Ok("Removed Crabdash from Rex's command palette.".into())
}

/// Brings an installed plugin up to this binary's, so `cargo install` updates
/// the palette without `rex install`. Only the binary the block names does
/// this; a development build run by hand leaves the installed plugin alone.
pub fn refresh() -> Result<()> {
    let config = config_path()?;
    let Ok(content) = fs::read_to_string(&config) else {
        return Ok(());
    };
    let Some((plugin, bin)) = installed(&content) else {
        return Ok(());
    };
    if fs::canonicalize(&bin).ok() != Some(this_binary()?) {
        return Ok(());
    }
    let next = with_block(&content, Some(&loader(&plugin, &bin)));
    if next == content && fs::read_to_string(&plugin).ok().as_deref() == Some(SOURCE) {
        return Ok(());
    }
    write_if_changed(&plugin, SOURCE)?;
    write_if_changed(&config, &next)?;
    // Not waited for, so starting the dashboard never stalls on Rex.
    let _ = Command::new(rex_program())
        .args(["config", "reload"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    Ok(())
}

fn this_binary() -> Result<PathBuf> {
    std::env::current_exe()
        .and_then(fs::canonicalize)
        .context("Could not find this crabdash-tui binary")
}

/// Rex's own lookup: `REX_CONFIG`, then `$XDG_CONFIG_HOME/rex`, then `~/.config/rex`.
fn config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("REX_CONFIG").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => dirs::home_dir()
            .context("Could not find the home directory")?
            .join(".config"),
    };
    Ok(base.join("rex").join("init.lua"))
}

fn plugin_path(config: &Path) -> PathBuf {
    config
        .parent()
        .unwrap_or(Path::new("."))
        .join("plugins")
        .join("crabdash")
        .join("init.lua")
}

/// The managed block. It names the plugin's version: Rex apps fetch actions
/// again only when init.lua changes, so a new plugin must change the block.
fn loader(plugin: &Path, bin: &Path) -> String {
    format!(
        "{BEGIN}\n-- Managed by `crabdash-tui rex install`; `crabdash-tui rex uninstall` removes it.\n\
         {PLUGIN}{:016x}\ndofile({}).setup({{ bin = {{ {} }} }})\n{END}\n",
        fnv1a(SOURCE),
        lua_string(&plugin.display().to_string()),
        lua_string(&bin.display().to_string()),
    )
}

/// The plugin file and binary an installed block names.
fn installed(content: &str) -> Option<(PathBuf, PathBuf)> {
    let start = content.find(BEGIN)?;
    let block = &content[start..start + content[start..].find(END)?];
    let after = |marker: &str| lua_unquote(&block[block.find(marker)? + marker.len()..]);
    Some((after("dofile(")?, after("bin = { ")?))
}

/// FNV-1a, 64 bits: stable across builds, unlike std's hasher.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn lua_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn lua_unquote(text: &str) -> Option<PathBuf> {
    let mut chars = text.strip_prefix('"')?.chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(PathBuf::from(out)),
            '\\' => out.push(chars.next()?),
            c => out.push(c),
        }
    }
    None
}

/// `content` with `block` where the managed block was, or appended. `None` removes it.
fn with_block(content: &str, block: Option<&str>) -> String {
    let replacement = block.unwrap_or_default();
    if let Some(start) = content.find(BEGIN)
        && let Some(length) = content[start..].find(END)
    {
        let after = &content[start + length + END.len()..];
        let after = after.strip_prefix('\n').unwrap_or(after);
        let mut before = &content[..start];
        if replacement.is_empty() {
            // Also drop the blank line that separated an appended block.
            before = before
                .strip_suffix("\n\n")
                .map_or(before, |b| &content[..b.len() + 1]);
        }
        return format!("{before}{replacement}{after}");
    }
    match (replacement.is_empty(), content.trim().is_empty()) {
        (true, _) => content.to_owned(),
        (false, true) => replacement.to_owned(),
        (false, false) => format!("{}\n\n{replacement}", content.trim_end_matches('\n')),
    }
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("Could not write {}", path.display()))
}

/// `rex` from PATH, or where Rex's Linux install script puts it.
fn rex_program() -> PathBuf {
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|dir| dir.join(if cfg!(windows) { "rex.exe" } else { "rex" }))
        .find(|path| path.is_file());
    on_path
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/bin/rex")))
        .unwrap_or_else(|| "rex".into())
}

fn rex(args: &[&str]) -> Result<()> {
    let output = Command::new(rex_program())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| anyhow!("Could not run rex: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let mut text = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    }
    bail!("rex {} failed: {text}", args.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_is_appended_replaced_and_removed_around_user_config() {
        let user = "rex.bind(\"cmd+b\", \"client.sidebar.toggle\")\n";
        let first = with_block(
            user,
            Some(&loader(Path::new("/p/a.lua"), Path::new("/b/x"))),
        );
        assert!(first.starts_with(user));
        let second = with_block(
            &format!("{first}-- after\n"),
            Some(&loader(Path::new("/p/b.lua"), Path::new("/b/x"))),
        );
        assert!(second.contains("/p/b.lua") && !second.contains("/p/a.lua"));
        assert!(second.ends_with("-- after\n"));
        assert_eq!(with_block(&first, None), user);
        assert_eq!(with_block(&second, None), format!("{user}-- after\n"));
        assert_eq!(with_block("", None), "");
    }

    #[test]
    fn the_installed_block_names_its_plugin_and_quoted_binary() {
        let block = loader(
            Path::new("/p/crab dash/init.lua"),
            Path::new("/bin/cra\"b\\tui"),
        );
        assert!(block.contains(&format!("{PLUGIN}{:016x}\n", fnv1a(SOURCE))));
        assert_eq!(
            installed(&format!("x = 1\n{block}")),
            Some((
                PathBuf::from("/p/crab dash/init.lua"),
                PathBuf::from("/bin/cra\"b\\tui")
            ))
        );
        assert_eq!(installed("no block"), None);
    }
}
