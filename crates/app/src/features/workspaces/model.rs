//! A workspace contains one ordered tab strip and one active feature.
use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Tab {
    Docker,
    Disks,
    Services,
}

impl Tab {
    pub(crate) const ALL: [Self; 3] = [Self::Docker, Self::Disks, Self::Services];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Layout {
    pub tabs: Vec<Tab>,
    pub active: Tab,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            tabs: Tab::ALL.to_vec(),
            active: Tab::Docker,
        }
    }
}

impl Layout {
    pub(crate) fn validate(&self) -> Result<()> {
        let unique: HashSet<_> = self.tabs.iter().copied().collect();
        if self.tabs.len() != Tab::ALL.len()
            || unique.len() != Tab::ALL.len()
            || !self.tabs.contains(&self.active)
        {
            bail!("Workspace must contain every feature exactly once and a valid active tab.");
        }
        Ok(())
    }

    pub(crate) fn active(&self) -> Tab {
        self.active
    }

    pub(crate) fn select(&mut self, tab: Tab) -> bool {
        if self.active == tab || !self.tabs.contains(&tab) {
            return false;
        }
        self.active = tab;
        true
    }

    pub(crate) fn show(&mut self, tab: Tab) -> bool {
        self.select(tab)
    }

    /// Insertion indices refer to boundaries in the original tab strip.
    /// Reordering preserves the selected feature, including when another tab moves.
    pub(crate) fn reorder(&mut self, tab: Tab, insertion_index: usize) -> bool {
        let Some(source) = self.tabs.iter().position(|current| *current == tab) else {
            return false;
        };
        let insertion_index = insertion_index.min(self.tabs.len());
        let destination = if source < insertion_index {
            insertion_index.saturating_sub(1)
        } else {
            insertion_index
        };
        if source == destination {
            return false;
        }
        self.tabs.remove(source);
        self.tabs.insert(destination, tab);
        true
    }
}

impl<'de> Deserialize<'de> for Layout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Flat {
            tabs: Vec<Tab>,
            active: Tab,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum SavedLayout {
            Flat(Flat),
            Legacy(LegacyLayout),
        }
        let layout = match SavedLayout::deserialize(deserializer)? {
            SavedLayout::Flat(flat) => Self {
                tabs: flat.tabs,
                active: flat.active,
            },
            SavedLayout::Legacy(legacy) => legacy.flatten().map_err(serde::de::Error::custom)?,
        };
        layout.validate().map_err(serde::de::Error::custom)?;
        Ok(layout)
    }
}

// Compatibility for version 1 files only. These types are never exposed to UI
// code or serialized: splits and detached windows become a single tab strip.
#[derive(Deserialize)]
struct LegacyLayout {
    root: LegacyNode,
    focused: u32,
    #[serde(default)]
    detached: Vec<LegacyWindow>,
}

#[derive(Deserialize)]
struct LegacyWindow {
    id: u32,
    node: LegacyNode,
    bounds: Option<LegacyBounds>,
}

#[derive(Deserialize)]
struct LegacyBounds {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacyAxis {
    Horizontal,
    Vertical,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LegacyNode {
    Pane {
        id: u32,
        tabs: Vec<Tab>,
        active: Tab,
    },
    Split {
        id: u32,
        #[serde(rename = "axis")]
        _axis: LegacyAxis,
        ratio: f32,
        first: Box<LegacyNode>,
        second: Box<LegacyNode>,
    },
}

impl LegacyLayout {
    fn flatten(self) -> Result<Layout> {
        fn visit(
            node: &LegacyNode,
            focused: u32,
            ids: &mut HashSet<u32>,
            tabs: &mut Vec<Tab>,
            active: &mut Option<Tab>,
            depth: usize,
        ) -> Result<()> {
            if depth > 3 {
                bail!("Workspace has too many nested splits.");
            }
            match node {
                LegacyNode::Pane {
                    id,
                    tabs: pane,
                    active: selected,
                } => {
                    if *id == 0 || !ids.insert(*id) || pane.is_empty() || !pane.contains(selected) {
                        bail!("Workspace has an invalid pane.");
                    }
                    tabs.extend(pane.iter().copied());
                    if *id == focused {
                        *active = Some(*selected);
                    }
                }
                LegacyNode::Split {
                    id,
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    if *id == 0
                        || !ids.insert(*id)
                        || !ratio.is_finite()
                        || !(0.15..=0.85).contains(ratio)
                    {
                        bail!("Workspace has an invalid split.");
                    }
                    visit(first, focused, ids, tabs, active, depth + 1)?;
                    visit(second, focused, ids, tabs, active, depth + 1)?;
                }
            }
            Ok(())
        }
        let mut ids = HashSet::new();
        let mut windows = HashSet::new();
        let mut tabs = Vec::new();
        let mut active = None;
        visit(
            &self.root,
            self.focused,
            &mut ids,
            &mut tabs,
            &mut active,
            0,
        )?;
        // First/second traversal preserves the old tree's visible ordering.
        // Detached windows follow the root in their persisted vector order.
        for window in &self.detached {
            if window.id == 0 || !windows.insert(window.id) {
                bail!("Duplicate detached window identifier.");
            }
            if let Some(bounds) = &window.bounds {
                if ![bounds.x, bounds.y, bounds.width, bounds.height]
                    .iter()
                    .all(|value| value.is_finite())
                    || bounds.x.abs() > 100_000.0
                    || bounds.y.abs() > 100_000.0
                    || !(320.0..=16_000.0).contains(&bounds.width)
                    || !(240.0..=16_000.0).contains(&bounds.height)
                {
                    bail!("Invalid workspace window bounds.");
                }
            }
            visit(
                &window.node,
                self.focused,
                &mut ids,
                &mut tabs,
                &mut active,
                0,
            )?;
        }
        let Some(active) = active else {
            bail!("Workspace is missing its focused pane.");
        };
        let layout = Layout { tabs, active };
        layout.validate()?;
        Ok(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reorder_boundary_preserves_features_and_selection() -> Result<()> {
        let mut permutations = Vec::new();
        for first in Tab::ALL {
            for second in Tab::ALL {
                for third in Tab::ALL {
                    if first != second && first != third && second != third {
                        permutations.push(vec![first, second, third]);
                    }
                }
            }
        }
        for tabs in permutations {
            for active in Tab::ALL {
                for tab in Tab::ALL {
                    for boundary in 0..=tabs.len() {
                        let mut layout = Layout {
                            tabs: tabs.clone(),
                            active,
                        };
                        layout.reorder(tab, boundary);
                        layout.validate()?;
                        assert_eq!(layout.active, active);
                        let before = &tabs[..boundary];
                        let after = &tabs[boundary..];
                        let expected: Vec<_> = before
                            .iter()
                            .copied()
                            .filter(|item| *item != tab)
                            .chain(std::iter::once(tab))
                            .chain(after.iter().copied().filter(|item| *item != tab))
                            .collect();
                        assert_eq!(layout.tabs, expected);
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn reordering_to_current_boundaries_is_a_noop_and_end_is_clamped() {
        let mut layout = Layout::default();
        assert!(!layout.reorder(Tab::Docker, 0));
        assert!(!layout.reorder(Tab::Docker, 1));
        assert!(layout.reorder(Tab::Docker, usize::MAX));
        assert_eq!(layout.tabs, vec![Tab::Disks, Tab::Services, Tab::Docker]);
        assert!(layout.select(Tab::Services));
        assert_eq!(layout.active(), Tab::Services);
        assert!(!layout.select(Tab::Services));
    }

    #[test]
    fn malformed_flat_layouts_are_rejected_at_deserialization() {
        for json in [
            r#"{"tabs":["docker","docker","services"],"active":"docker"}"#,
            r#"{"tabs":["docker","disks"],"active":"services"}"#,
            r#"{"tabs":[],"active":"docker"}"#,
            r#"{"tabs":["docker","disks","unknown"],"active":"docker"}"#,
        ] {
            assert!(serde_json::from_str::<Layout>(json).is_err());
        }
    }

    #[test]
    fn old_split_and_detached_layouts_flatten_and_preserve_focused_selection() -> Result<()> {
        let old = r#"{
            "root":{"kind":"split","id":4,"axis":"horizontal","ratio":0.5,
              "first":{"kind":"pane","id":1,"tabs":["disks"],"active":"disks"},
              "second":{"kind":"pane","id":2,"tabs":["docker"],"active":"docker"}},
            "focused":3,
            "detached":[{"id":3,"node":{"kind":"pane","id":3,"tabs":["services"],"active":"services"},
               "bounds":{"x":-700,"y":90,"width":640,"height":480}}]
        }"#;
        let layout: Layout = serde_json::from_str(old)?;
        assert_eq!(layout.tabs, vec![Tab::Disks, Tab::Docker, Tab::Services]);
        assert_eq!(layout.active, Tab::Services);
        let encoded = serde_json::to_string(&layout)?;
        assert!(!encoded.contains("root"));
        assert!(!encoded.contains("detached"));
        assert_eq!(serde_json::from_str::<Layout>(&encoded)?, layout);
        Ok(())
    }

    #[test]
    fn legacy_corruption_is_rejected_instead_of_silently_repaired() -> Result<()> {
        let old = serde_json::json!({
            "root":{"kind":"pane","id":1,"tabs":["docker","disks","services"],"active":"docker"},
            "focused":1
        });
        let mut missing_focus = old.clone();
        missing_focus["focused"] = serde_json::json!(99);
        let mut duplicate = old.clone();
        duplicate["root"]["tabs"] = serde_json::json!(["docker", "docker", "services"]);
        let mut invalid_active = old.clone();
        invalid_active["root"]["tabs"] = serde_json::json!(["docker", "disks"]);
        invalid_active["root"]["active"] = serde_json::json!("services");
        for corrupt in [missing_focus, duplicate, invalid_active] {
            assert!(serde_json::from_value::<Layout>(corrupt).is_err());
        }
        let valid: Layout = serde_json::from_value(old)?;
        valid.validate()
    }
}
