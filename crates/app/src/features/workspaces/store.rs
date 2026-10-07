//! Workspace persistence shares the platform's application configuration directory.
use super::model::Layout;
use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

// Version 4 adds System. Versions 1/2 migrate to a
// single pane; version 3 preserves splits. Loading never rewrites the file.
const VERSION: u32 = 4;

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
#[serde(try_from = "SavedStore")]
pub(crate) struct Store {
    pub version: u32,
    pub active: Uuid,
    pub workspaces: Vec<Workspace>,
}

#[derive(Deserialize)]
struct SavedStore {
    version: u32,
    active: Uuid,
    workspaces: Vec<SavedWorkspace>,
}

#[derive(Deserialize)]
struct SavedWorkspace {
    id: Uuid,
    name: String,
    layout: serde_json::Value,
    sidebar_collapsed: bool,
    sidebar_width: f32,
    terminal_open: bool,
}

impl TryFrom<SavedStore> for Store {
    type Error = anyhow::Error;

    fn try_from(saved: SavedStore) -> Result<Self> {
        if !(1..=VERSION).contains(&saved.version) {
            bail!("Unsupported workspace file version.");
        }
        let mut workspaces = Vec::with_capacity(saved.workspaces.len());
        for workspace in saved.workspaces {
            workspaces.push(Workspace {
                id: workspace.id,
                name: workspace.name,
                layout: Layout::from_saved(workspace.layout, saved.version)?,
                sidebar_collapsed: workspace.sidebar_collapsed,
                sidebar_width: workspace.sidebar_width,
                terminal_open: workspace.terminal_open,
            });
        }
        let store = Self {
            version: VERSION,
            active: saved.active,
            workspaces,
        };
        store.validate()?;
        Ok(store)
    }
}

impl Default for Store {
    fn default() -> Self {
        let id = Uuid::new_v4();
        Self {
            version: VERSION,
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

    pub(crate) fn load(default_sidebar_width: f32) -> Result<Self> {
        Self::load_from(&Self::path()?, default_sidebar_width)
    }

    fn load_from(path: &Path, default_sidebar_width: f32) -> Result<Self> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut store = Self::default();
                store.current_mut().sidebar_width = default_sidebar_width;
                store.validate()?;
                return Ok(store);
            }
            Err(error) => return Err(error).context("Unable to load workspaces"),
        };
        let store: Self = serde_json::from_slice(&bytes).context("Unable to read workspaces")?;
        store.validate()?;
        Ok(store)
    }

    pub(crate) fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        fs::create_dir_all(path.parent().context("Missing workspace directory")?)?;
        let temporary = path.with_extension(format!("json.{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            return Err(error).context("Unable to save workspaces");
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.version != VERSION || self.workspaces.is_empty() || self.workspaces.len() > 24 {
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

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("crabdash-workspace-test-{}", Uuid::new_v4())))
        }
        fn path(&self) -> PathBuf {
            self.0.join("workspaces.json")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

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

    #[test]
    fn first_workspace_migrates_sidebar_preference_without_creating_a_file() -> Result<()> {
        let fixture = Fixture::new();
        let path = fixture.path();
        let store = Store::load_from(&path, 318.0)?;
        assert_eq!(store.current().sidebar_width, 318.0);
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn tab_orders_and_session_preferences_survive_save_switch_and_reload() -> Result<()> {
        use super::super::model::Tab;
        let fixture = Fixture::new();
        let path = fixture.path();
        let mut store = Store::load_from(&path, 286.0)?;
        let first_id = store.active;
        assert!(store.current_mut().layout.drop_tab(
            Tab::Services,
            1,
            super::super::model::Drop::Tab(0)
        ));
        store.current_mut().terminal_open = true;
        store.current_mut().sidebar_collapsed = true;
        let saved = store.current().clone();
        let mut second = saved.clone();
        second.id = Uuid::new_v4();
        second.name = "Storage".into();
        assert!(
            second
                .layout
                .drop_tab(Tab::Disks, 1, super::super::model::Drop::Tab(0))
        );
        let second_layout = second.layout.clone();
        store.active = second.id;
        store.workspaces.push(second);
        store.save_to(&path)?;

        let mut reloaded = Store::load_from(&path, 200.0)?;
        assert_eq!(reloaded.current().layout, second_layout);
        reloaded.active = first_id;
        assert_eq!(reloaded.current().layout, saved.layout);
        assert!(reloaded.current().terminal_open);
        assert!(reloaded.current().sidebar_collapsed);
        assert_eq!(reloaded.current().sidebar_width, 286.0);
        reloaded.current_mut().name = "Services".into();
        reloaded.save_to(&path)?;
        let restarted = Store::load_from(&path, 240.0)?;
        assert_eq!(restarted.active, first_id);
        assert_eq!(restarted.current().name, "Services");
        assert_eq!(restarted.current().layout, saved.layout);
        Ok(())
    }

    #[test]
    fn version_one_migrates_without_rewriting_until_save() -> Result<()> {
        use super::super::model::Tab;
        let fixture = Fixture::new();
        let path = fixture.path();
        fs::create_dir_all(&fixture.0)?;
        let id = Uuid::new_v4();
        let old = serde_json::json!({
            "version":1, "active":id, "workspaces":[{
                "id":id, "name":"Original", "sidebar_collapsed":true,
                "sidebar_width":318.0, "terminal_open":true,
                "layout":{
                    "root":{"kind":"pane","id":1,"tabs":["disks","docker"],"active":"disks"},
                    "focused":2,
                    "detached":[{"id":2,"node":{"kind":"pane","id":2,"tabs":["services"],"active":"services"},"bounds":null}]
                }
            }]
        });
        let original = serde_json::to_vec_pretty(&old)?;
        fs::write(&path, &original)?;
        let store = Store::load_from(&path, 240.0)?;
        assert_eq!(fs::read(&path)?, original);
        assert_eq!(store.version, VERSION);
        assert_eq!(store.active, id);
        assert_eq!(
            store
                .current()
                .layout
                .pane(1)
                .map(|(tabs, _)| tabs.to_vec()),
            Some(vec![Tab::Disks, Tab::Docker, Tab::Services, Tab::System])
        );
        assert_eq!(store.current().layout.active(), Tab::Services);
        assert!(store.current().sidebar_collapsed);
        assert!(store.current().terminal_open);
        assert_eq!(store.current().sidebar_width, 318.0);
        store.save_to(&path)?;
        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(written["version"], VERSION);
        assert!(written["workspaces"][0]["layout"].get("root").is_some());
        assert!(written["workspaces"][0]["layout"].get("detached").is_none());
        assert_eq!(
            Store::load_from(&path, 200.0)?.current().layout,
            store.current().layout
        );
        Ok(())
    }

    #[test]
    fn version_two_preserves_flat_order_selection_and_file_bytes() -> Result<()> {
        use super::super::model::{Node, Tab};
        let fixture = Fixture::new();
        let path = fixture.path();
        fs::create_dir_all(&fixture.0)?;
        let id = Uuid::new_v4();
        let saved = serde_json::json!({
            "version":2, "active":id, "workspaces":[{
                "id":id, "name":"Flat", "sidebar_collapsed":false,
                "sidebar_width":286.0, "terminal_open":true,
                "layout":{"tabs":["services","docker","disks"],"active":"disks"}
            }]
        });
        let original = serde_json::to_vec_pretty(&saved)?;
        fs::write(&path, &original)?;
        let store = Store::load_from(&path, 240.0)?;
        assert_eq!(fs::read(&path)?, original);
        assert_eq!(store.version, VERSION);
        assert_eq!(
            store.current().layout.pane(1),
            Some((
                &[Tab::Services, Tab::Docker, Tab::Disks, Tab::System][..],
                Tab::Disks
            ))
        );
        assert!(matches!(
            store.current().layout.root,
            Node::Pane { id: 1, .. }
        ));
        store.save_to(&path)?;
        let rewritten: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(rewritten["version"], VERSION);
        assert!(rewritten["workspaces"][0]["layout"].get("tabs").is_none());
        assert_eq!(
            Store::load_from(&path, 240.0)?.current().layout,
            store.current().layout
        );
        Ok(())
    }

    #[test]
    fn legacy_split_and_detached_content_migrate_to_one_ordered_pane() -> Result<()> {
        use super::super::model::{Node, Tab};
        let id = Uuid::new_v4();
        let saved = serde_json::json!({
            "version":1, "active":id, "workspaces":[{
                "id":id, "name":"Legacy", "sidebar_collapsed":false,
                "sidebar_width":240.0, "terminal_open":false,
                "layout":{
                    "root":{"kind":"split","id":4,"axis":"horizontal","ratio":0.6,
                        "first":{"kind":"pane","id":1,"tabs":["disks"],"active":"disks"},
                        "second":{"kind":"pane","id":2,"tabs":["docker"],"active":"docker"}},
                    "focused":3,
                    "detached":[{"id":3,"node":{"kind":"pane","id":3,"tabs":["services"],"active":"services"},"bounds":null}]
                }
            }]
        });
        let store: Store = serde_json::from_value(saved.clone())?;
        assert_eq!(
            store.current().layout.pane(1),
            Some((
                &[Tab::Disks, Tab::Docker, Tab::Services, Tab::System][..],
                Tab::Services
            ))
        );
        assert!(matches!(store.current().layout.root, Node::Pane { .. }));
        for corrupt in [
            {
                let mut value = saved.clone();
                value["workspaces"][0]["layout"]["focused"] = serde_json::json!(99);
                value
            },
            {
                let mut value = saved.clone();
                value["workspaces"][0]["layout"]["root"]["ratio"] = serde_json::json!(1.0);
                value
            },
            {
                let mut value = saved.clone();
                value["workspaces"][0]["layout"]["root"]["second"]["id"] = serde_json::json!(1);
                value
            },
            {
                let mut value = saved.clone();
                value["workspaces"][0]["layout"]["detached"][0]["bounds"] =
                    serde_json::json!({"x":0.0,"y":0.0,"width":1.0,"height":1.0});
                value
            },
        ] {
            assert!(serde_json::from_value::<Store>(corrupt).is_err());
        }
        Ok(())
    }

    #[test]
    fn invalid_legacy_features_and_future_versions_preserve_original_files() -> Result<()> {
        let fixture = Fixture::new();
        let path = fixture.path();
        fs::create_dir_all(&fixture.0)?;
        let id = Uuid::new_v4();
        let mut saved = serde_json::json!({
            "version":1, "active":id, "workspaces":[{
                "id":id, "name":"Original", "sidebar_collapsed":false,
                "sidebar_width":240.0, "terminal_open":false,
                "layout":{"root":{"kind":"pane","id":1,"tabs":["docker","docker","services"],"active":"docker"},"focused":1}
            }]
        });
        let invalid = serde_json::to_vec(&saved)?;
        fs::write(&path, &invalid)?;
        assert!(Store::load_from(&path, 240.0).is_err());
        assert_eq!(fs::read(&path)?, invalid);
        saved["workspaces"][0]["layout"] = serde_json::to_value(Layout::default())?;
        saved["version"] = serde_json::json!(99);
        let future = serde_json::to_vec(&saved)?;
        fs::write(&path, &future)?;
        assert!(Store::load_from(&path, 240.0).is_err());
        assert_eq!(fs::read(&path)?, future);
        Ok(())
    }

    #[test]
    fn version_three_preserves_splits_and_appends_system_only_to_focused_pane() -> Result<()> {
        use super::super::model::Tab;
        let fixture = Fixture::new();
        let path = fixture.path();
        fs::create_dir_all(&fixture.0)?;
        let id = Uuid::new_v4();
        let saved = serde_json::json!({
            "version":3,"active":id,"workspaces":[{
                "id":id,"name":"Split","sidebar_collapsed":false,"sidebar_width":240.0,"terminal_open":false,
                "layout":{"root":{"kind":"split","id":3,"axis":"horizontal","ratio":0.7,
                    "first":{"kind":"pane","id":1,"tabs":["services","docker"],"active":"docker"},
                    "second":{"kind":"pane","id":2,"tabs":["disks"],"active":"disks"}},"focused":2}
            }]
        });
        let bytes = serde_json::to_vec_pretty(&saved)?;
        fs::write(&path, &bytes)?;
        let store = Store::load_from(&path, 240.0)?;
        assert_eq!(fs::read(&path)?, bytes);
        assert_eq!(store.current().layout.focused, 2);
        assert_eq!(store.current().layout.active(), Tab::Disks);
        assert_eq!(
            store.current().layout.pane(1),
            Some((&[Tab::Services, Tab::Docker][..], Tab::Docker))
        );
        assert_eq!(
            store.current().layout.pane(2),
            Some((&[Tab::Disks, Tab::System][..], Tab::Disks))
        );
        assert_eq!(
            serde_json::to_value(&store.current().layout)?["root"]["ratio"],
            serde_json::json!(0.7f32)
        );
        let mut bad = saved;
        bad["workspaces"][0]["layout"]["root"]["second"]["tabs"] = serde_json::json!(["system"]);
        assert!(serde_json::from_value::<Store>(bad).is_err());
        store.save_to(&path)?;
        assert_eq!(
            Store::load_from(&path, 240.0)?.current().layout,
            store.current().layout
        );
        Ok(())
    }

    #[test]
    fn corrupt_files_remain_intact_and_invalid_updates_do_not_replace_valid_files() -> Result<()> {
        let fixture = Fixture::new();
        let path = fixture.path();
        fs::create_dir_all(&fixture.0)?;
        let corrupt = b"{broken workspace data";
        fs::write(&path, corrupt)?;
        assert!(Store::load_from(&path, 240.0).is_err());
        assert_eq!(fs::read(&path)?, corrupt);
        let mut store = Store::default();
        store.save_to(&path)?;
        let valid = fs::read(&path)?;
        store.current_mut().sidebar_width = f32::NAN;
        assert!(store.save_to(&path).is_err());
        assert_eq!(fs::read(&path)?, valid);
        Ok(())
    }
}
