use anyhow::{Context as _, Result};
use std::{env, path::PathBuf};
pub(super) fn path() -> Result<PathBuf> {
    let directory =
        PathBuf::from(env::var_os("HOME").context("Unable to find your home directory")?)
            .join("Library/Application Support/Crabdash");
    Ok(directory.join("preferences.json"))
}
