//! Workspace persistence shares the platform's application configuration directory.
use super::model::Layout;
use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::PathBuf};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub layout: Layout,
    pub sidebar_collapsed: bool,
    pub sidebar_width: f32,
    pub terminal_open: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Store {
    pub version: u32,
    pub active: Uuid,
    pub workspaces: Vec<Workspace>,
}

impl Default for Store {
    fn default() -> Self {
        let id = Uuid::new_v4();
        Self {
            version: 1,
            active: id,
            workspaces: vec![Workspace {
                id,
                name: "Default".into(),
                layout: Layout::default(),
                sidebar_collapsed: false,
                sidebar_width: 240.0,
                terminal_open: false,
            }],
        }
    }
}

impl Store {
    fn path() -> Result<PathBuf> {
        let settings = crate::features::preferences::Preferences::path()?;
        Ok(settings
            .parent()
            .context("Missing configuration directory")?
            .join("workspaces.json"))
    }

    pub(crate) fn load() -> Result<Self> {
        let bytes = match fs::read(Self::path()?) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error).context("Unable to load workspaces"),
        };
        let store: Self = serde_json::from_slice(&bytes).context("Unable to read workspaces")?;
        store.validate()?;
        Ok(store)
    }

    pub(crate) fn save(&self) -> Result<()> {
        self.validate()?;
        let path = Self::path()?;
        fs::create_dir_all(path.parent().context("Missing workspace directory")?)?;
        let temporary = path.with_extension(format!("json.{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&temporary, &path).context("Unable to save workspaces")
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.version != 1 || self.workspaces.is_empty() || self.workspaces.len() > 24 {
            bail!("Unsupported workspace file.");
        }
        let mut ids = HashSet::new();
        for workspace in &self.workspaces {
            validate_name(&workspace.name)?;
            if !ids.insert(workspace.id) {
                bail!("Duplicate workspace identifier.");
            }
            if !workspace.sidebar_width.is_finite()
                || !(180.0..=420.0).contains(&workspace.sidebar_width)
            {
                bail!("Invalid workspace sidebar size.");
            }
            workspace.layout.validate()?;
        }
        if !ids.contains(&self.active) {
            bail!("Active workspace is missing.");
        }
        Ok(())
    }

    pub(crate) fn current(&self) -> &Workspace {
        // State is constructed only from a validated store or its nonempty default.
        self.workspaces
            .iter()
            .find(|workspace| workspace.id == self.active)
            .map_or(&self.workspaces[0], |workspace| workspace)
    }

    pub(crate) fn current_mut(&mut self) -> &mut Workspace {
        let index = self
            .workspaces
            .iter()
            .position(|workspace| workspace.id == self.active)
            .map_or(0, |index| index);
        &mut self.workspaces[index]
    }
}

pub(crate) fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.chars().count() > 48 || name.chars().any(char::is_control) {
        bail!("Use a name between 1 and 48 characters.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_layouts_round_trip_and_validate() -> Result<()> {
        let store = Store::default();
        store.validate()?;
        let decoded: Store = serde_json::from_slice(&serde_json::to_vec(&store)?)?;
        decoded.validate()?;
        assert_eq!(decoded.current().layout, store.current().layout);
        assert_eq!(decoded.active, store.active);
        Ok(())
    }
    #[test]
    fn invalid_active_identifiers_names_and_versions_are_rejected() {
        let mut store = Store::default();
        store.active = Uuid::new_v4();
        assert!(store.validate().is_err());
        store = Store::default();
        store.workspaces[0].name = "\n".into();
        assert!(store.validate().is_err());
        store = Store::default();
        store.version = 9;
        assert!(store.validate().is_err());
    }
}
