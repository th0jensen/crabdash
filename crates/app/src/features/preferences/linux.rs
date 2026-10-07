use anyhow::{Context as _, Result};
use std::{env, path::PathBuf};
pub(super) fn path() -> Result<PathBuf> {
    let directory = match env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        Some(path) => path.join("crabdash"),
        None => PathBuf::from(env::var_os("HOME").context("Unable to find your home directory")?)
            .join(".config/crabdash"),
    };
    Ok(directory.join("preferences.json"))
}
