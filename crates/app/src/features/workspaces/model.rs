//! Serializable docking tree. A feature belongs to exactly one pane.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Node {
    Pane {
        id: u32,
        tabs: Vec<Tab>,
        active: Tab,
    },
    Split {
        id: u32,
        axis: Axis,
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Layout {
    pub root: Node,
    pub focused: u32,
    #[serde(default)]
    pub detached: Vec<Detached>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Detached {
    pub id: u32,
    pub node: Node,
    pub bounds: Option<WindowRect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct WindowRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drop {
    Tab(usize),
    Left,
    Right,
    Top,
    Bottom,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            root: Node::Pane {
                id: 1,
                tabs: Tab::ALL.to_vec(),
                active: Tab::Docker,
            },
            focused: 1,
            detached: Vec::new(),
        }
    }
}

impl Node {
    pub(crate) fn pane(&self, target: u32) -> Option<(&[Tab], Tab)> {
        match self {
            Self::Pane { id, tabs, active } if *id == target => Some((tabs, *active)),
            Self::Split { first, second, .. } => first.pane(target).or_else(|| second.pane(target)),
            _ => None,
        }
    }

    fn pane_mut(&mut self, target: u32) -> Option<(&mut Vec<Tab>, &mut Tab)> {
        match self {
            Self::Pane { id, tabs, active } if *id == target => Some((tabs, active)),
            Self::Split { first, second, .. } => {
                first.pane_mut(target).or_else(|| second.pane_mut(target))
            }
            _ => None,
        }
    }

    fn containing(&self, tab: Tab) -> Option<u32> {
        match self {
            Self::Pane { id, tabs, .. } => tabs.contains(&tab).then_some(*id),
            Self::Split { first, second, .. } => {
                first.containing(tab).or_else(|| second.containing(tab))
            }
        }
    }

    fn maximum_id(&self) -> u32 {
        match self {
            Self::Pane { id, .. } => *id,
            Self::Split {
                id, first, second, ..
            } => (*id).max(first.maximum_id()).max(second.maximum_id()),
        }
    }

    fn first_pane(&self) -> u32 {
        match self {
            Self::Pane { id, .. } => *id,
            Self::Split { first, .. } => first.first_pane(),
        }
    }

    fn split(&mut self, target: u32, tab: Tab, drop: Drop, new_id: u32) -> bool {
        match self {
            Self::Pane { id, .. } if *id == target => {
                let old = self.clone();
                let new = Self::Pane {
                    id: new_id,
                    tabs: vec![tab],
                    active: tab,
                };
                let (first, second) = if matches!(drop, Drop::Left | Drop::Top) {
                    (new, old)
                } else {
                    (old, new)
                };
                *self = Self::Split {
                    id: new_id + 1,
                    axis: if matches!(drop, Drop::Left | Drop::Right) {
                        Axis::Horizontal
                    } else {
                        Axis::Vertical
                    },
                    ratio: 0.5,
                    first: Box::new(first),
                    second: Box::new(second),
                };
                true
            }
            Self::Split { first, second, .. } => {
                first.split(target, tab, drop, new_id) || second.split(target, tab, drop, new_id)
            }
            _ => false,
        }
    }

    fn compact(self) -> Option<Self> {
        match self {
            Self::Pane { ref tabs, .. } if tabs.is_empty() => None,
            Self::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => match (first.compact(), second.compact()) {
                (Some(first), Some(second)) => Some(Self::Split {
                    id,
                    axis,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (first, second) => first.or(second),
            },
            pane => Some(pane),
        }
    }

    pub(crate) fn set_ratio(&mut self, target: u32, ratio: f32) {
        match self {
            Self::Split {
                id, ratio: current, ..
            } if *id == target => *current = ratio.clamp(0.15, 0.85),
            Self::Split { first, second, .. } => {
                first.set_ratio(target, ratio);
                second.set_ratio(target, ratio);
            }
            _ => {}
        }
    }
}

impl Layout {
    pub(crate) fn validate(&self) -> Result<()> {
        fn visit(
            node: &Node,
            ids: &mut HashSet<u32>,
            tabs_seen: &mut HashSet<Tab>,
            depth: usize,
        ) -> Result<()> {
            if depth > 3 {
                bail!("Workspace has too many nested splits.");
            }
            match node {
                Node::Pane { id, tabs, active } => {
                    if !ids.insert(*id) || *id == 0 || tabs.is_empty() || !tabs.contains(active) {
                        bail!("Workspace has an invalid pane.");
                    }
                    for tab in tabs {
                        if !tabs_seen.insert(*tab) {
                            bail!("Workspace contains duplicate tabs.");
                        }
                    }
                }
                Node::Split {
                    id,
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    if !ids.insert(*id)
                        || *id == 0
                        || !ratio.is_finite()
                        || !(0.15..=0.85).contains(ratio)
                    {
                        bail!("Workspace has an invalid split.");
                    }
                    visit(first, ids, tabs_seen, depth + 1)?;
                    visit(second, ids, tabs_seen, depth + 1)?;
                }
            }
            Ok(())
        }
        let mut ids = HashSet::new();
        let mut tabs = HashSet::new();
        visit(&self.root, &mut ids, &mut tabs, 0)?;
        let mut windows = HashSet::new();
        for detached in &self.detached {
            if detached.id == 0 || !windows.insert(detached.id) {
                bail!("Duplicate detached window identifier.");
            }
            visit(&detached.node, &mut ids, &mut tabs, 0)?;
            if let Some(bounds) = detached.bounds {
                if ![bounds.x, bounds.y, bounds.width, bounds.height]
                    .iter()
                    .all(|value| value.is_finite())
                    || bounds.width < 320.0
                    || bounds.height < 240.0
                    || bounds.width > 16000.0
                    || bounds.height > 16000.0
                {
                    bail!("Invalid workspace window bounds.");
                }
            }
        }
        if tabs.len() != Tab::ALL.len() || self.pane(self.focused).is_none() {
            bail!("Workspace is missing tabs or its focused pane.");
        }
        Ok(())
    }

    pub(crate) fn pane(&self, id: u32) -> Option<(&[Tab], Tab)> {
        self.root
            .pane(id)
            .or_else(|| self.detached.iter().find_map(|window| window.node.pane(id)))
    }

    fn pane_mut(&mut self, id: u32) -> Option<(&mut Vec<Tab>, &mut Tab)> {
        if self.root.pane(id).is_some() {
            self.root.pane_mut(id)
        } else {
            self.detached
                .iter_mut()
                .find_map(|window| window.node.pane_mut(id))
        }
    }

    fn containing(&self, tab: Tab) -> Option<u32> {
        self.root.containing(tab).or_else(|| {
            self.detached
                .iter()
                .find_map(|window| window.node.containing(tab))
        })
    }

    fn maximum_id(&self) -> u32 {
        self.detached
            .iter()
            .map(|window| window.node.maximum_id().max(window.id))
            .fold(self.root.maximum_id(), u32::max)
    }

    pub(crate) fn detach(&mut self, tab: Tab) -> Option<u32> {
        let source = self.root.containing(tab)?;
        let mut candidate = self.clone();
        let id = candidate.maximum_id().checked_add(1)?;
        let (tabs, active) = candidate.root.pane_mut(source)?;
        tabs.retain(|current| *current != tab);
        if *active == tab {
            if let Some(next) = tabs.first() {
                *active = *next;
            }
        }
        candidate.root = candidate.root.compact()?;
        candidate.detached.push(Detached {
            id,
            node: Node::Pane {
                id,
                tabs: vec![tab],
                active: tab,
            },
            bounds: None,
        });
        candidate.focused = id;
        if candidate.validate().is_err() {
            return None;
        }
        *self = candidate;
        Some(id)
    }

    pub(crate) fn move_back(&mut self, window_id: u32) -> bool {
        let Some(index) = self
            .detached
            .iter()
            .position(|window| window.id == window_id)
        else {
            return false;
        };
        let mut tabs = Vec::new();
        fn collect(node: &Node, tabs: &mut Vec<Tab>) {
            match node {
                Node::Pane {
                    tabs: pane_tabs, ..
                } => tabs.extend(pane_tabs),
                Node::Split { first, second, .. } => {
                    collect(first, tabs);
                    collect(second, tabs);
                }
            }
        }
        collect(&self.detached[index].node, &mut tabs);
        let target = self.root.first_pane();
        let mut changed = false;
        for tab in tabs {
            changed |= self.move_tab(tab, target, Drop::Tab(usize::MAX));
        }
        changed
    }

    pub(crate) fn set_bounds(&mut self, window_id: u32, bounds: WindowRect) {
        if let Some(window) = self
            .detached
            .iter_mut()
            .find(|window| window.id == window_id)
        {
            window.bounds = Some(bounds);
        }
    }

    pub(crate) fn set_ratio(&mut self, split: u32, ratio: f32) {
        if !ratio.is_finite() {
            return;
        }
        self.root.set_ratio(split, ratio);
        for window in &mut self.detached {
            window.node.set_ratio(split, ratio);
        }
    }

    pub(crate) fn active(&self) -> Tab {
        self.pane(self.focused)
            .map_or(Tab::Docker, |(_, active)| active)
    }

    pub(crate) fn select(&mut self, pane: u32, tab: Tab) -> bool {
        if let Some((tabs, active)) = self.pane_mut(pane) {
            if tabs.contains(&tab) {
                *active = tab;
                self.focused = pane;
                return true;
            }
        }
        false
    }

    pub(crate) fn show(&mut self, tab: Tab) {
        if let Some(pane) = self.containing(tab) {
            self.select(pane, tab);
        }
    }

    pub(crate) fn move_tab(&mut self, tab: Tab, target: u32, drop: Drop) -> bool {
        let Some(source) = self.containing(tab) else {
            return false;
        };
        let Some((target_tabs, _)) = self.pane(target) else {
            return false;
        };
        if source == target && target_tabs.len() == 1 {
            return false;
        }
        let adjusted_drop = match drop {
            Drop::Tab(index) if source == target => {
                let source_index = target_tabs
                    .iter()
                    .position(|current| *current == tab)
                    .map_or(0, |index| index);
                Drop::Tab(if source_index < index {
                    index.saturating_sub(1)
                } else {
                    index
                })
            }
            other => other,
        };
        let new_id = self.maximum_id().saturating_add(1);
        if new_id >= u32::MAX - 1 {
            return false;
        }
        let mut candidate = self.clone();
        if let Some((tabs, active)) = candidate.pane_mut(source) {
            tabs.retain(|current| *current != tab);
            if *active == tab {
                if let Some(next) = tabs.first() {
                    *active = *next;
                }
            }
        }
        match adjusted_drop {
            Drop::Tab(index) => {
                if let Some((tabs, active)) = candidate.pane_mut(target) {
                    tabs.insert(index.min(tabs.len()), tab);
                    *active = tab;
                }
                candidate.focused = target;
            }
            edge => {
                if !candidate.root.split(target, tab, edge, new_id) {
                    for detached in &mut candidate.detached {
                        if detached.node.split(target, tab, edge, new_id) {
                            break;
                        }
                    }
                }
                candidate.focused = new_id;
            }
        }
        let Some(root) = candidate.root.compact() else {
            return false;
        };
        candidate.root = root;
        candidate.detached = candidate
            .detached
            .into_iter()
            .filter_map(|mut detached| {
                detached.node = detached.node.compact()?;
                Some(detached)
            })
            .collect();
        if candidate.pane(candidate.focused).is_none() {
            candidate.focused = candidate.root.first_pane();
        }
        if candidate.validate().is_err() {
            return false;
        }
        *self = candidate;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_merge_and_reorder_preserve_all_tabs() -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.move_tab(Tab::Services, 1, Drop::Right));
        let services_pane = layout.focused;
        assert!(layout.move_tab(Tab::Disks, services_pane, Drop::Bottom));
        let disks_pane = layout.focused;
        layout.validate()?;
        assert!(layout.move_tab(Tab::Services, disks_pane, Drop::Tab(0)));
        assert!(layout.move_tab(Tab::Docker, disks_pane, Drop::Tab(1)));
        assert_eq!(
            layout.root.pane(disks_pane).map(|(tabs, _)| tabs.to_vec()),
            Some(vec![Tab::Services, Tab::Docker, Tab::Disks])
        );
        layout.validate()?;
        let encoded = serde_json::to_vec(&layout)?;
        assert_eq!(serde_json::from_slice::<Layout>(&encoded)?, layout);
        Ok(())
    }

    #[test]
    fn invalid_moves_are_transactional() {
        let mut layout = Layout::default();
        let original = layout.clone();
        assert!(!layout.move_tab(Tab::Docker, 99, Drop::Left));
        assert_eq!(layout, original);
        assert!(layout.move_tab(Tab::Docker, 1, Drop::Left));
        let split = layout.clone();
        assert!(!layout.move_tab(Tab::Docker, layout.focused, Drop::Right));
        assert_eq!(layout, split);
    }

    #[test]
    fn reordering_uses_the_destination_position_before_removal() -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.move_tab(Tab::Docker, 1, Drop::Tab(2)));
        assert_eq!(
            layout.root.pane(1).map(|(tabs, _)| tabs.to_vec()),
            Some(vec![Tab::Disks, Tab::Docker, Tab::Services])
        );
        layout.validate()
    }

    #[test]
    fn detached_windows_move_back_without_losing_features() -> Result<()> {
        let mut layout = Layout::default();
        let Some(window) = layout.detach(Tab::Services) else {
            bail!("Expected a detached services window");
        };
        layout.set_bounds(
            window,
            WindowRect {
                x: -800.0,
                y: 60.0,
                width: 640.0,
                height: 480.0,
            },
        );
        assert_eq!(layout.detached.len(), 1);
        layout.validate()?;
        assert!(layout.move_tab(Tab::Disks, window, Drop::Bottom));
        layout.validate()?;
        assert!(layout.move_back(window));
        assert!(layout.detached.is_empty());
        layout.validate()?;
        assert_eq!(layout.root.pane(1).map(|(tabs, _)| tabs.len()), Some(3));
        Ok(())
    }

    #[test]
    fn the_last_main_pane_cannot_be_detached() -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.detach(Tab::Services).is_some());
        assert!(layout.detach(Tab::Disks).is_some());
        let original = layout.clone();
        assert!(layout.detach(Tab::Docker).is_none());
        assert_eq!(layout, original);
        layout.validate()
    }

    #[test]
    fn corrupt_panes_and_missing_features_are_rejected() {
        let mut layout = Layout::default();
        if let Node::Pane { tabs, active, .. } = &mut layout.root {
            tabs.pop();
            *active = Tab::Services;
        }
        assert!(layout.validate().is_err());
        layout = Layout::default();
        if let Node::Pane { tabs, .. } = &mut layout.root {
            tabs.push(Tab::Docker);
        }
        assert!(layout.validate().is_err());
        layout = Layout::default();
        layout.focused = 99;
        assert!(layout.validate().is_err());
    }
}
