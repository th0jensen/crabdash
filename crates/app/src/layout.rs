//! Shared in-window pane tree; feature and terminal layouts use the same moves.
pub(crate) mod geometry;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Node<T> {
    Pane {
        id: u32,
        tabs: Vec<T>,
        active: T,
    },
    Split {
        id: u32,
        axis: Axis,
        ratio: f32,
        first: Box<Node<T>>,
        second: Box<Node<T>>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Layout<T> {
    pub root: Node<T>,
    pub focused: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drop {
    Tab(usize),
    Left,
    Right,
    Top,
    Bottom,
}

impl<T: Copy + Eq> Node<T> {
    pub(crate) fn pane(&self, target: u32) -> Option<(&[T], T)> {
        match self {
            Self::Pane { id, tabs, active } if *id == target => Some((tabs, *active)),
            Self::Split { first, second, .. } => first.pane(target).or_else(|| second.pane(target)),
            _ => None,
        }
    }
    pub(crate) fn pane_mut(&mut self, target: u32) -> Option<(&mut Vec<T>, &mut T)> {
        match self {
            Self::Pane { id, tabs, active } if *id == target => Some((tabs, active)),
            Self::Split { first, second, .. } => {
                first.pane_mut(target).or_else(|| second.pane_mut(target))
            }
            _ => None,
        }
    }
    fn containing(&self, tab: T) -> Option<u32> {
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
    fn split(&mut self, target: u32, tab: T, edge: Drop, pane_id: u32, split_id: u32) -> bool {
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

impl<T: Copy + Eq + std::hash::Hash> Layout<T> {
    pub(crate) fn single(tabs: Vec<T>, active: T) -> Self {
        Self {
            root: Node::Pane {
                id: 1,
                tabs,
                active,
            },
            focused: 1,
        }
    }
    pub(crate) fn validate_members(&self, expected: &[T]) -> Result<()> {
        fn visit<T: Copy + Eq + std::hash::Hash>(
            node: &Node<T>,
            ids: &mut HashSet<u32>,
            tabs_seen: &mut HashSet<T>,
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
    pub(crate) fn pane(&self, target: u32) -> Option<(&[T], T)> {
        self.root.pane(target)
    }
    pub(crate) fn focused_tab(&self) -> Option<T> {
        self.pane(self.focused).map(|(_, active)| active)
    }
    pub(crate) fn focus(&mut self, pane: u32) -> bool {
        if self.focused == pane || self.pane(pane).is_none() {
            return false;
        }
        self.focused = pane;
        true
    }
    pub(crate) fn select(&mut self, tab: T) -> bool {
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
    pub(crate) fn show(&mut self, tab: T) -> bool {
        self.select(tab)
    }
    pub(crate) fn set_ratio(&mut self, split: u32, ratio: f32) -> bool {
        if !ratio.is_finite() {
            return false;
        }
        self.root.set_ratio(split, ratio.clamp(0.15, 0.85))
    }
    /// T indices are insertion boundaries in the original destination strip.
    /// Edges split that pane and select the moved feature, in this same window.
    pub(crate) fn drop_tab(&mut self, tab: T, target: u32, drop: Drop) -> bool {
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
        if candidate == *self || candidate.validate_members(&self.tabs()).is_err() {
            return false;
        }
        *self = candidate;
        true
    }
    pub(crate) fn tabs(&self) -> Vec<T> {
        fn collect<T: Copy>(node: &Node<T>, out: &mut Vec<T>) {
            match node {
                Node::Pane { tabs, .. } => out.extend(tabs.iter().copied()),
                Node::Split { first, second, .. } => {
                    collect(first, out);
                    collect(second, out);
                }
            }
        }
        let mut tabs = Vec::new();
        collect(&self.root, &mut tabs);
        tabs
    }
    pub(crate) fn insert(&mut self, tab: T) -> bool {
        if self.root.containing(tab).is_some() {
            return false;
        }
        let Some((tabs, active)) = self.root.pane_mut(self.focused) else {
            return false;
        };
        tabs.push(tab);
        *active = tab;
        true
    }
    /// The caller owns the empty-drawer policy; a layout always retains a pane.
    pub(crate) fn remove(&mut self, tab: T) -> bool {
        let Some(source) = self.root.containing(tab) else {
            return false;
        };
        let identities = self.tabs();
        if identities.len() == 1 {
            return false;
        }
        let group = self
            .pane(source)
            .filter(|(tabs, _)| tabs.len() == 1)
            .and_then(|_| self.root.parent_axis(source))
            .and_then(|axis| {
                self.root
                    .surviving_group_anchor(source, axis)
                    .map(|id| (id, axis))
            });
        let mut candidate = self.clone();
        let Some((tabs, active)) = candidate.root.pane_mut(source) else {
            return false;
        };
        let Some(index) = tabs.iter().position(|item| *item == tab) else {
            return false;
        };
        tabs.remove(index);
        if *active == tab && !tabs.is_empty() {
            *active = tabs[index.min(tabs.len() - 1)];
        }
        let Some(root) = candidate.root.compact() else {
            return false;
        };
        candidate.root = root;
        if let Some((anchor, axis)) = group {
            candidate.root.equalize_group_containing(anchor, axis);
        }
        if candidate.pane(candidate.focused).is_none() {
            let Some(remaining) = identities.iter().copied().find(|item| *item != tab) else {
                return false;
            };
            let Some(pane) = candidate.root.containing(remaining) else {
                return false;
            };
            candidate.focused = pane;
        }
        let expected: Vec<T> = identities.into_iter().filter(|item| *item != tab).collect();
        if candidate.validate_members(&expected).is_err() {
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
    fn dynamic_sessions_split_join_and_close_without_losing_identity() -> Result<()> {
        let a = uuid::Uuid::new_v4();
        let b = uuid::Uuid::new_v4();
        let c = uuid::Uuid::new_v4();
        let mut layout = Layout::single(vec![a], a);
        assert!(layout.insert(b));
        assert!(!layout.insert(b));
        assert!(layout.insert(c));
        assert!(layout.drop_tab(b, 1, Drop::Right));
        let right = layout.focused;
        assert_eq!(layout.focused_tab(), Some(b));
        assert!(layout.drop_tab(c, right, Drop::Bottom));
        let lower = layout.focused;
        layout.validate_members(&[a, b, c])?;
        assert!(layout.drop_tab(b, lower, Drop::Tab(0)));
        assert!(layout.remove(b));
        assert_eq!(layout.focused_tab(), Some(c));
        assert!(layout.remove(c));
        assert_eq!(layout.focused_tab(), Some(a));
        assert!(matches!(layout.root, Node::Pane { id: 1, .. }));
        assert!(!layout.remove(a));
        assert!(!layout.remove(b));
        layout.validate_members(&[a])
    }

    #[test]
    fn closing_an_inactive_tab_keeps_focus_and_active_shell() -> Result<()> {
        let mut layout = Layout::single(vec![10, 20, 30], 20);
        assert!(layout.remove(10));
        assert_eq!(layout.pane(1), Some((&[20, 30][..], 20)));
        assert!(layout.remove(20));
        assert_eq!(layout.focused_tab(), Some(30));
        layout.validate_members(&[30])
    }

    #[test]
    fn unknown_or_invalid_drops_leave_the_whole_tree_unchanged() {
        let mut layout = Layout::single(vec![10, 20], 10);
        let before = layout.clone();
        assert!(!layout.drop_tab(99, 1, Drop::Left));
        assert!(!layout.drop_tab(10, 99, Drop::Right));
        assert!(!layout.remove(99));
        assert!(!layout.focus(99));
        assert!(!layout.set_ratio(99, f32::NAN));
        assert_eq!(layout, before);
    }
}
