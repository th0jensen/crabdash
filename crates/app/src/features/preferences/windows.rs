//! User-scoped preferences follow the Windows roaming application directory.
use anyhow::{Context as _, Result};
use std::{env, path::PathBuf};

pub(super) fn path() -> Result<PathBuf> {
    let directory = PathBuf::from(
        env::var_os("APPDATA")
            .context("Unable to locate your Windows application data directory")?,
    );
    Ok(directory.join("Crabdash/preferences.json"))
}
