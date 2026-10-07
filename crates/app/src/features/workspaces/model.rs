//! A workspace is one window containing an ordered tree of feature panes.
use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Tab {
    Docker,
    Disks,
    Services,
    System,
}
impl Tab {
    pub(crate) const ALL: [Self; 4] = [Self::Docker, Self::Disks, Self::Services, Self::System];
    const LEGACY: [Self; 3] = [Self::Docker, Self::Disks, Self::Services];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
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

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Layout {
    pub root: Node,
    pub focused: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedTree {
    root: Node,
    focused: u32,
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
        Self::single(Tab::ALL.to_vec(), Tab::Docker)
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
    fn split(&mut self, target: u32, tab: Tab, edge: Drop, pane_id: u32, split_id: u32) -> bool {
        match self {
            Self::Pane { id, .. } if *id == target => {
                let old = self.clone();
                let new = Self::Pane {
                    id: pane_id,
                    tabs: vec![tab],
                    active: tab,
                };
                let (first, second) = if matches!(edge, Drop::Left | Drop::Top) {
                    (new, old)
                } else {
                    (old, new)
                };
                *self = Self::Split {
                    id: split_id,
                    axis: if matches!(edge, Drop::Left | Drop::Right) {
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
                first.split(target, tab, edge, pane_id, split_id)
                    || second.split(target, tab, edge, pane_id, split_id)
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
    fn set_ratio(&mut self, target: u32, ratio: f32) -> bool {
        match self {
            Self::Split {
                id, ratio: current, ..
            } if *id == target => {
                if *current == ratio {
                    return false;
                }
                *current = ratio;
                true
            }
            Self::Split { first, second, .. } => {
                first.set_ratio(target, ratio) || second.set_ratio(target, ratio)
            }
            _ => false,
        }
    }

    fn axis_group_contains(&self, target: u32, axis: Axis) -> bool {
        let id = match self {
            Self::Pane { id, .. } | Self::Split { id, .. } => *id,
        };
        if id == target {
            return true;
        }
        match self {
            Self::Split {
                axis: current,
                first,
                second,
                ..
            } if *current == axis => {
                first.axis_group_contains(target, axis) || second.axis_group_contains(target, axis)
            }
            _ => false,
        }
    }

    fn parent_axis(&self, pane: u32) -> Option<Axis> {
        match self {
            Self::Split {
                axis,
                first,
                second,
                ..
            } => {
                if matches!(first.as_ref(), Self::Pane { id, .. } if *id == pane)
                    || matches!(second.as_ref(), Self::Pane { id, .. } if *id == pane)
                {
                    Some(*axis)
                } else {
                    first.parent_axis(pane).or_else(|| second.parent_axis(pane))
                }
            }
            _ => None,
        }
    }

    /// An opposite-axis subtree is one member, identified by its root rather
    /// than a descendant pane, so removal cannot resize an unrelated inner group.
    fn other_axis_member(&self, removed: u32, axis: Axis) -> Option<u32> {
        match self {
            Self::Split {
                axis: current,
                first,
                second,
                ..
            } if *current == axis => first
                .other_axis_member(removed, axis)
                .or_else(|| second.other_axis_member(removed, axis)),
            Self::Pane { id, .. } | Self::Split { id, .. } => (*id != removed).then_some(*id),
        }
    }

    fn surviving_group_anchor(&self, removed: u32, axis: Axis) -> Option<u32> {
        if self.axis_group_contains(removed, axis) {
            return self.other_axis_member(removed, axis);
        }
        match self {
            Self::Split { first, second, .. } => first
                .surviving_group_anchor(removed, axis)
                .or_else(|| second.surviving_group_anchor(removed, axis)),
            _ => None,
        }
    }

    fn axis_group_size(&self, axis: Axis) -> usize {
        match self {
            Self::Split {
                axis: current,
                first,
                second,
                ..
            } if *current == axis => first.axis_group_size(axis) + second.axis_group_size(axis),
            _ => 1,
        }
    }

    fn equalize_axis_group(&mut self, axis: Axis) {
        if let Self::Split {
            axis: current,
            ratio,
            first,
            second,
            ..
        } = self
            && *current == axis
        {
            let first_size = first.axis_group_size(axis);
            *ratio = first_size as f32 / (first_size + second.axis_group_size(axis)) as f32;
            first.equalize_axis_group(axis);
            second.equalize_axis_group(axis);
        }
    }

    fn equalize_group_containing(&mut self, target: u32, axis: Axis) -> bool {
        if self.axis_group_contains(target, axis) {
            self.equalize_axis_group(axis);
            return true;
        }
        match self {
            Self::Split { first, second, .. } => {
                first.equalize_group_containing(target, axis)
                    || second.equalize_group_containing(target, axis)
            }
            _ => false,
        }
    }
}

impl Layout {
    fn single(tabs: Vec<Tab>, active: Tab) -> Self {
        Self {
            root: Node::Pane {
                id: 1,
                tabs,
                active,
            },
            focused: 1,
        }
    }
    pub(crate) fn from_saved(value: serde_json::Value, version: u32) -> Result<Self> {
        let layout = match version {
            1 => serde_json::from_value::<LegacyLayout>(value)?.flatten()?,
            2 => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Flat {
                    tabs: Vec<Tab>,
                    active: Tab,
                }
                let flat: Flat = serde_json::from_value(value)?;
                Self::single(flat.tabs, flat.active)
            }
            3 => {
                let saved: SavedTree = serde_json::from_value(value)?;
                Self {
                    root: saved.root,
                    focused: saved.focused,
                }
            }
            4 => serde_json::from_value(value)?,
            _ => bail!("Unsupported workspace layout version."),
        };
        let mut layout = layout;
        if version < 4 {
            layout.validate_features(&Tab::LEGACY)?;
            let focused = layout.focused;
            let Some((tabs, _)) = layout.root.pane_mut(focused) else {
                bail!("Workspace is missing its focused pane.");
            };
            tabs.push(Tab::System);
        }
        layout.validate()?;
        Ok(layout)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        self.validate_features(&Tab::ALL)
    }
    fn validate_features(&self, expected: &[Tab]) -> Result<()> {
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
                    if *id == 0 || !ids.insert(*id) || tabs.is_empty() || !tabs.contains(active) {
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
                    if *id == 0
                        || !ids.insert(*id)
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
        if tabs.len() != expected.len()
            || expected.iter().any(|tab| !tabs.contains(tab))
            || self.pane(self.focused).is_none()
        {
            bail!("Workspace is missing features or its focused pane.");
        }
        Ok(())
    }
    pub(crate) fn pane(&self, target: u32) -> Option<(&[Tab], Tab)> {
        self.root.pane(target)
    }
    pub(crate) fn active(&self) -> Tab {
        self.pane(self.focused)
            .map_or(Tab::Docker, |(_, active)| active)
    }
    pub(crate) fn focus(&mut self, pane: u32) -> bool {
        if self.focused == pane || self.pane(pane).is_none() {
            return false;
        }
        self.focused = pane;
        true
    }
    pub(crate) fn select(&mut self, tab: Tab) -> bool {
        let Some(pane) = self.root.containing(tab) else {
            return false;
        };
        let Some((_, active)) = self.root.pane_mut(pane) else {
            return false;
        };
        let changed = self.focused != pane || *active != tab;
        *active = tab;
        self.focused = pane;
        changed
    }
    pub(crate) fn show(&mut self, tab: Tab) -> bool {
        self.select(tab)
    }
    pub(crate) fn set_ratio(&mut self, split: u32, ratio: f32) -> bool {
        if !ratio.is_finite() {
            return false;
        }
        self.root.set_ratio(split, ratio.clamp(0.15, 0.85))
    }
    /// Tab indices are insertion boundaries in the original destination strip.
    /// Edges split that pane and select the moved feature, in this same window.
    pub(crate) fn drop_tab(&mut self, tab: Tab, target: u32, drop: Drop) -> bool {
        let Some(source) = self.root.containing(tab) else {
            return false;
        };
        let Some((target_tabs, _)) = self.pane(target) else {
            return false;
        };
        if source == target && target_tabs.len() == 1 {
            return false;
        }
        let source_group = self
            .pane(source)
            .filter(|(tabs, _)| tabs.len() == 1)
            .and_then(|_| self.root.parent_axis(source))
            .and_then(|axis| {
                self.root
                    .surviving_group_anchor(source, axis)
                    .map(|anchor| (anchor, axis))
            });
        let drop = match drop {
            Drop::Tab(index) => {
                let index = index.min(target_tabs.len());
                let source_index = target_tabs.iter().position(|current| *current == tab);
                Drop::Tab(
                    if source == target
                        && source_index.is_some_and(|source_index| source_index < index)
                    {
                        index - 1
                    } else {
                        index
                    },
                )
            }
            edge => edge,
        };
        let mut candidate = self.clone();
        let Some((tabs, active)) = candidate.root.pane_mut(source) else {
            return false;
        };
        let Some(source_index) = tabs.iter().position(|current| *current == tab) else {
            return false;
        };
        tabs.remove(source_index);
        if *active == tab && !tabs.is_empty() {
            *active = tabs[source_index.min(tabs.len() - 1)];
        }
        let mut new_split = None;
        match drop {
            Drop::Tab(index) => {
                let Some((tabs, active)) = candidate.root.pane_mut(target) else {
                    return false;
                };
                tabs.insert(index.min(tabs.len()), tab);
                *active = tab;
                candidate.focused = target;
            }
            edge => {
                let Some(pane_id) = self.root.maximum_id().checked_add(1) else {
                    return false;
                };
                let Some(split_id) = pane_id.checked_add(1) else {
                    return false;
                };
                if !candidate.root.split(target, tab, edge, pane_id, split_id) {
                    return false;
                }
                new_split = Some((
                    split_id,
                    if matches!(edge, Drop::Left | Drop::Right) {
                        Axis::Horizontal
                    } else {
                        Axis::Vertical
                    },
                ));
                candidate.focused = pane_id;
            }
        }
        let Some(root) = candidate.root.compact() else {
            return false;
        };
        candidate.root = root;
        if let Some((anchor, axis)) = source_group {
            let anchor = if anchor == target {
                new_split.map_or(anchor, |(split, _)| split)
            } else {
                anchor
            };
            candidate.root.equalize_group_containing(anchor, axis);
        }
        if let Some((split, axis)) = new_split {
            candidate.root.equalize_group_containing(split, axis);
        }
        if candidate == *self || candidate.validate().is_err() {
            return false;
        }
        *self = candidate;
        true
    }
}

impl<'de> Deserialize<'de> for Layout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let saved = SavedTree::deserialize(deserializer)?;
        let layout = Self {
            root: saved.root,
            focused: saved.focused,
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
        let layout = Layout::single(tabs, active);
        layout.validate_features(&Tab::LEGACY)?;
        Ok(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_edge_creates_the_expected_axis_and_order() -> Result<()> {
        for (edge, expected_axis, moved_first) in [
            (Drop::Left, Axis::Horizontal, true),
            (Drop::Right, Axis::Horizontal, false),
            (Drop::Top, Axis::Vertical, true),
            (Drop::Bottom, Axis::Vertical, false),
        ] {
            let mut layout = Layout::default();
            assert!(layout.drop_tab(Tab::Services, 1, edge));
            layout.validate()?;
            let Node::Split {
                axis,
                ratio,
                first,
                second,
                ..
            } = &layout.root
            else {
                bail!("Expected an in-window split");
            };
            assert_eq!(*axis, expected_axis);
            assert_eq!(*ratio, 0.5);
            let moved = if moved_first { first } else { second };
            assert_eq!(
                moved.pane(layout.focused),
                Some((&[Tab::Services][..], Tab::Services))
            );
            assert_eq!(layout.active(), Tab::Services);
            assert!(!layout.drop_tab(Tab::Services, layout.focused, edge));
            let json = serde_json::to_string(&layout)?;
            assert!(!json.contains("detached"));
            assert_eq!(serde_json::from_str::<Layout>(&json)?, layout);
        }
        Ok(())
    }

    #[test]
    fn cross_pane_moves_collapse_empty_panes_and_keep_target_order() -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        let services_pane = layout.focused;
        assert!(layout.drop_tab(Tab::Disks, services_pane, Drop::Bottom));
        let disks_pane = layout.focused;
        assert!(layout.drop_tab(Tab::Services, disks_pane, Drop::Tab(0)));
        assert!(layout.pane(services_pane).is_none());
        assert!(layout.drop_tab(Tab::System, disks_pane, Drop::Tab(usize::MAX)));
        assert!(layout.drop_tab(Tab::Docker, disks_pane, Drop::Tab(1)));
        assert!(layout.pane(1).is_none());
        assert_eq!(
            layout.pane(disks_pane),
            Some((
                &[Tab::Services, Tab::Docker, Tab::Disks, Tab::System][..],
                Tab::Docker
            ))
        );
        assert!(matches!(layout.root, Node::Pane { id, .. } if id == disks_pane));
        assert_eq!(layout.focused, disks_pane);
        layout.validate()
    }

    #[test]
    fn all_original_insertion_boundaries_keep_every_feature_once() -> Result<()> {
        fn permutations(tabs: Vec<Tab>, remaining: Vec<Tab>, result: &mut Vec<Vec<Tab>>) {
            if remaining.is_empty() {
                result.push(tabs);
                return;
            }
            for (index, tab) in remaining.iter().copied().enumerate() {
                let mut next = tabs.clone();
                next.push(tab);
                let mut remaining = remaining.clone();
                remaining.remove(index);
                permutations(next, remaining, result);
            }
        }
        let mut orders = Vec::new();
        permutations(Vec::new(), Tab::ALL.to_vec(), &mut orders);
        for tabs in orders {
            for tab in Tab::ALL {
                for boundary in 0..=tabs.len() {
                    let mut layout = Layout::single(tabs.clone(), tab);
                    layout.drop_tab(tab, 1, Drop::Tab(boundary));
                    let expected: Vec<_> = tabs[..boundary]
                        .iter()
                        .copied()
                        .filter(|item| *item != tab)
                        .chain(std::iter::once(tab))
                        .chain(tabs[boundary..].iter().copied().filter(|item| *item != tab))
                        .collect();
                    assert_eq!(layout.pane(1), Some((expected.as_slice(), tab)));
                    layout.validate()?;
                }
            }
        }
        Ok(())
    }

    #[test]
    fn invalid_moves_are_transactional_and_current_boundaries_are_noops() -> Result<()> {
        let mut layout = Layout::default();
        let original = layout.clone();
        assert!(!layout.drop_tab(Tab::Docker, 1, Drop::Tab(0)));
        assert!(!layout.drop_tab(Tab::Docker, 1, Drop::Tab(1)));
        assert!(!layout.drop_tab(Tab::Docker, 99, Drop::Left));
        assert_eq!(layout, original);
        assert!(layout.drop_tab(Tab::Docker, 1, Drop::Tab(usize::MAX)));
        assert_eq!(
            layout.pane(1),
            Some((
                &[Tab::Disks, Tab::Services, Tab::System, Tab::Docker][..],
                Tab::Docker
            ))
        );
        if let Node::Pane { id, .. } = &mut layout.root {
            *id = u32::MAX;
        }
        layout.focused = u32::MAX;
        let before_overflow = layout.clone();
        assert!(!layout.drop_tab(Tab::Services, u32::MAX, Drop::Right));
        assert_eq!(layout, before_overflow);
        layout.validate()
    }

    #[test]
    fn focus_selection_and_ratio_changes_validate() -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        let services_pane = layout.focused;
        let Node::Split { id: split, .. } = layout.root else {
            bail!("Expected split");
        };
        assert!(!layout.set_ratio(split, f32::NAN));
        assert!(!layout.set_ratio(split, f32::INFINITY));
        assert!(!layout.set_ratio(99, 0.6));
        assert!(!layout.set_ratio(split, 0.5));
        assert!(layout.set_ratio(split, -1.0));
        assert!(matches!(layout.root, Node::Split { ratio: 0.15, .. }));
        assert!(layout.set_ratio(split, 1.0));
        assert!(matches!(layout.root, Node::Split { ratio: 0.85, .. }));
        assert!(layout.focus(1));
        assert!(!layout.focus(1));
        assert!(!layout.focus(99));
        assert_eq!(layout.active(), Tab::Docker);
        assert!(layout.select(Tab::Disks));
        assert_eq!(layout.active(), Tab::Disks);
        assert!(layout.show(Tab::Services));
        assert_eq!(layout.focused, services_pane);
        assert!(!layout.select(Tab::Services));
        layout.validate()
    }

    #[test]
    fn a_third_same_axis_pane_equalizes_the_connected_group() -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        let services_pane = layout.focused;
        let Node::Split { id: split, .. } = layout.root else {
            bail!("Expected split");
        };
        assert!(layout.set_ratio(split, 0.7));
        assert!(layout.drop_tab(Tab::Disks, services_pane, Drop::Right));
        let Node::Split { ratio, second, .. } = &layout.root else {
            bail!("Expected split");
        };
        assert!((*ratio - 1.0 / 3.0).abs() < f32::EPSILON);
        let Node::Split { ratio: inner, .. } = second.as_ref() else {
            bail!("Expected nested split");
        };
        assert_eq!(*inner, 0.5);
        layout.validate()
    }

    #[test]
    fn a_new_opposite_axis_group_preserves_existing_custom_ratios() -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        let services_pane = layout.focused;
        let Node::Split { id: split, .. } = layout.root else {
            bail!("Expected split");
        };
        assert!(layout.set_ratio(split, 0.7));
        assert!(layout.drop_tab(Tab::Disks, services_pane, Drop::Bottom));
        let Node::Split {
            axis,
            ratio,
            second,
            ..
        } = &layout.root
        else {
            bail!("Expected split");
        };
        assert_eq!(*axis, Axis::Horizontal);
        assert_eq!(*ratio, 0.7);
        assert!(matches!(
            second.as_ref(),
            Node::Split {
                axis: Axis::Vertical,
                ratio: 0.5,
                ..
            }
        ));
        layout.validate()
    }

    #[test]
    fn removing_one_of_three_siblings_equalizes_the_surviving_panes() -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        let services = layout.focused;
        assert!(layout.drop_tab(Tab::Disks, services, Drop::Right));
        let disks = layout.focused;
        assert!(layout.drop_tab(Tab::Disks, 1, Drop::Tab(1)));
        assert!(layout.pane(disks).is_none());
        assert!(matches!(
            layout.root,
            Node::Split {
                axis: Axis::Horizontal,
                ratio: 0.5,
                ..
            }
        ));
        assert_eq!(
            layout.pane(1),
            Some((&[Tab::Docker, Tab::Disks, Tab::System][..], Tab::Disks))
        );
        layout.validate()
    }

    #[test]
    fn collapse_does_not_equalize_the_surviving_opposite_axis_subtree() -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        let services = layout.focused;
        assert!(layout.drop_tab(Tab::Disks, services, Drop::Bottom));
        let disks = layout.focused;
        let Node::Split { second, .. } = &layout.root else {
            bail!("Expected split");
        };
        let Node::Split { id: vertical, .. } = second.as_ref() else {
            bail!("Expected vertical split");
        };
        let vertical = *vertical;
        assert!(layout.set_ratio(vertical, 0.7));
        assert!(layout.drop_tab(Tab::System, disks, Drop::Tab(usize::MAX)));
        assert!(layout.drop_tab(Tab::Docker, disks, Drop::Tab(0)));
        assert!(
            matches!(layout.root, Node::Split { id, axis: Axis::Vertical, ratio: 0.7, .. } if id == vertical)
        );
        layout.validate()
    }

    #[test]
    fn replacing_the_source_group_anchor_with_an_opposite_split_still_equalizes_siblings()
    -> Result<()> {
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        let services = layout.focused;
        assert!(layout.drop_tab(Tab::Disks, services, Drop::Right));
        // The middle pane disappears and the surviving anchor pane1 becomes
        // an opposite-axis pair, which still counts as one horizontal member.
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Top));
        assert!(layout.pane(services).is_none());
        let Node::Split {
            axis, ratio, first, ..
        } = &layout.root
        else {
            bail!("Expected remaining horizontal group");
        };
        assert_eq!(*axis, Axis::Horizontal);
        assert_eq!(*ratio, 0.5);
        assert!(matches!(
            first.as_ref(),
            Node::Split {
                axis: Axis::Vertical,
                ratio: 0.5,
                ..
            }
        ));
        layout.validate()
    }

    #[test]
    fn corrupt_tree_and_legacy_data_are_rejected() -> Result<()> {
        let valid = serde_json::to_value(Layout::default())?;
        for (field, value) in [
            ("focused", serde_json::json!(99)),
            ("focused", serde_json::json!(0)),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(serde_json::from_value::<Layout>(invalid).is_err());
        }
        let mut duplicate = valid.clone();
        duplicate["root"]["tabs"] = serde_json::json!(["docker", "docker", "services"]);
        assert!(serde_json::from_value::<Layout>(duplicate.clone()).is_err());
        assert!(Layout::from_saved(duplicate, 1).is_err());
        let mut legacy = valid.clone();
        legacy["detached"] = serde_json::json!([]);
        assert!(serde_json::from_value::<Layout>(legacy.clone()).is_err());
        legacy["root"]["tabs"] = serde_json::json!(["docker", "disks", "services"]);
        assert!(Layout::from_saved(legacy, 1).is_ok());
        let mut invalid_active = valid.clone();
        invalid_active["root"]["tabs"] = serde_json::json!(["docker", "disks"]);
        invalid_active["root"]["active"] = serde_json::json!("services");
        assert!(Layout::from_saved(invalid_active, 1).is_err());
        Ok(())
    }
}
