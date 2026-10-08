//! Session identities remain in the drawer while its shared pane tree moves.
use crate::layout::{Drop, Layout};
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct Model {
    pub scope: Uuid,
    pub revision: u64,
    pub layout: Layout<Uuid>,
    pub drag_target: Option<(u32, Drop)>,
}

impl Model {
    pub fn new(session: Uuid) -> Self {
        Self {
            scope: Uuid::new_v4(),
            revision: 0,
            layout: Layout::single(vec![session], session),
            drag_target: None,
        }
    }

    fn changed(&mut self, changed: bool) -> bool {
        if changed {
            self.revision = self.revision.wrapping_add(1);
            self.drag_target = None;
        }
        changed
    }

    #[cfg(test)]
    fn insert(&mut self, session: Uuid) -> bool {
        let changed = self.layout.insert(session);
        self.changed(changed)
    }

    pub fn remove(&mut self, session: Uuid) -> bool {
        let changed = self.layout.remove(session);
        self.changed(changed)
    }

    pub fn drop_tab(&mut self, session: Uuid, pane: u32, drop: Drop) -> bool {
        let changed = self.layout.drop_tab(session, pane, drop);
        self.changed(changed)
    }

    /// Plan before allocating an emulator, input entity, or transport.
    pub fn with_session(&self, session: Uuid, edge: Option<Drop>) -> Option<Self> {
        let mut candidate = self.clone();
        let old_active = self.layout.focused_tab()?;
        let pane = self.layout.focused;
        if !candidate.layout.insert(session) {
            return None;
        }
        if let Some(edge) = edge {
            if !matches!(edge, Drop::Right | Drop::Bottom) {
                return None;
            }
            candidate.layout.select(old_active);
            if !candidate.layout.drop_tab(session, pane, edge) {
                return None;
            }
        }
        candidate.changed(true);
        Some(candidate)
    }

    /// Navigate the focused pane's current order without changing topology.
    pub fn adjacent(&self, forward: bool) -> Option<Uuid> {
        let (tabs, active) = self.layout.pane(self.layout.focused)?;
        if tabs.len() < 2 {
            return None;
        }
        let index = tabs.iter().position(|session| *session == active)?;
        let next = if forward {
            if index == tabs.len() - 1 {
                0
            } else {
                index + 1
            }
        } else if index == 0 {
            tabs.len() - 1
        } else {
            index - 1
        };
        tabs.get(next).copied()
    }

    pub fn accepts(
        &self,
        scope: Uuid,
        revision: u64,
        pane: u32,
        index: usize,
        session: Uuid,
    ) -> bool {
        self.scope == scope
            && self.revision == revision
            && self
                .layout
                .pane(pane)
                .is_some_and(|(tabs, _)| tabs.get(index) == Some(&session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_splits_preserve_the_original_active_session() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let third = Uuid::new_v4();
        let fresh = Uuid::new_v4();
        for tabs in [vec![first], vec![first, second, third]] {
            let mut model = Model::new(first);
            model.layout = Layout::single(tabs.clone(), first);
            for edge in [Drop::Right, Drop::Bottom] {
                let candidate = model.with_session(fresh, Some(edge)).expect("valid split");
                assert_eq!(candidate.layout.pane(1), Some((tabs.as_slice(), first)));
                assert_eq!(candidate.layout.focused_tab(), Some(fresh));
                assert_eq!(candidate.revision, model.revision + 1);
                assert_eq!(model.layout, Layout::single(tabs.clone(), first));
            }
        }
    }

    #[test]
    fn rejected_fresh_splits_do_not_change_the_live_layout_or_identity() {
        let mut model = Model::new(Uuid::new_v4());
        for edge in [Drop::Right, Drop::Bottom, Drop::Right] {
            model = model
                .with_session(Uuid::new_v4(), Some(edge))
                .expect("depth allowed");
        }
        let layout = model.layout.clone();
        let revision = model.revision;
        let fresh = Uuid::new_v4();
        assert!(model.with_session(fresh, Some(Drop::Bottom)).is_none());
        assert!(model.with_session(fresh, Some(Drop::Tab(0))).is_none());
        assert_eq!(model.layout, layout);
        assert_eq!(model.revision, revision);
        assert!(!model.layout.tabs().contains(&fresh));
    }

    #[test]
    fn sessions_reorder_split_join_and_close_without_changing_identity() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let third = Uuid::new_v4();
        let mut model = Model::new(first);
        assert!(model.insert(second));
        assert!(model.insert(third));
        assert!(model.drop_tab(first, 1, Drop::Tab(3)));
        assert_eq!(
            model.layout.pane(1).map(|(tabs, _)| tabs.to_vec()),
            Some(vec![second, third, first])
        );
        assert!(model.drop_tab(first, 1, Drop::Right));
        assert_eq!(model.layout.focused_tab(), Some(first));
        assert!(
            model
                .layout
                .validate_members(&[first, second, third])
                .is_ok()
        );
        assert!(model.drop_tab(first, 1, Drop::Tab(0)));
        assert!(model.remove(second));
        assert!(model.remove(first));
        assert_eq!(model.layout.focused_tab(), Some(third));
        assert!(!model.remove(third));
        assert!(model.layout.validate_members(&[third]).is_ok());
    }

    #[test]
    fn drag_scope_and_revision_reject_closed_or_recreated_sources() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut model = Model::new(first);
        let scope = model.scope;
        assert!(model.accepts(scope, 0, 1, 0, first));
        assert!(!model.accepts(Uuid::new_v4(), 0, 1, 0, first));
        assert!(model.insert(second));
        assert!(!model.accepts(scope, 0, 1, 0, first));
        assert!(model.accepts(scope, 1, 1, 0, first));
        assert!(model.remove(first));
        assert!(!model.accepts(scope, 1, 1, 0, first));
        let replacement = Model::new(first);
        assert!(!replacement.accepts(scope, 0, 1, 0, first));
    }

    #[test]
    fn adjacent_sessions_follow_reordered_tabs_and_wrap_without_mutation() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let third = Uuid::new_v4();
        let mut model = Model::new(first);
        assert!(model.insert(second));
        assert!(model.insert(third));
        assert!(model.drop_tab(first, 1, Drop::Tab(3)));
        let revision = model.revision;
        assert_eq!(model.adjacent(true), Some(second));
        assert_eq!(model.adjacent(false), Some(third));
        assert!(model.layout.select(second));
        assert_eq!(model.adjacent(true), Some(third));
        assert_eq!(model.adjacent(false), Some(first));
        assert_eq!(model.revision, revision);
        assert_eq!(model.layout.tabs(), vec![second, third, first]);
    }

    #[test]
    fn navigation_stays_in_the_focused_split_and_rejects_single_or_missing_panes() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let third = Uuid::new_v4();
        let mut model = Model::new(first);
        assert_eq!(model.adjacent(true), None);
        assert!(model.insert(second));
        assert!(model.insert(third));
        assert!(model.drop_tab(first, 1, Drop::Right));
        assert_eq!(model.adjacent(true), None);
        assert_eq!(model.adjacent(false), None);
        assert!(model.layout.select(second));
        assert_eq!(model.adjacent(true), Some(third));
        assert_eq!(model.adjacent(false), Some(third));
        model.layout.focused = 99;
        assert_eq!(model.adjacent(true), None);
        model.layout = Layout::single(Vec::new(), first);
        assert_eq!(model.adjacent(false), None);
    }
}
