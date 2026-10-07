//! A drag is stamped when it starts, after synchronizing its source window.
use crate::features::workspaces::model::{Layout, Tab};
use uuid::Uuid;

pub(super) struct Source {
    pub(super) workspace: Uuid,
    pub(super) pane: u32,
    pub(super) index: usize,
    pub(super) tab: Tab,
}

impl Source {
    pub(super) fn stamp(&self, workspace: Uuid, revision: u64, layout: &Layout) -> Option<u64> {
        (self.workspace == workspace
            && layout
                .pane(self.pane)
                .is_some_and(|(tabs, _)| tabs.get(self.index) == Some(&self.tab)))
        .then_some(revision)
    }

    pub(super) fn accepts(
        &self,
        stamp: Option<u64>,
        workspace: Uuid,
        revision: u64,
        layout: &Layout,
    ) -> bool {
        stamp == Some(revision) && self.stamp(workspace, revision, layout) == stamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::workspaces::model::Drop;

    fn source(workspace: Uuid) -> Source {
        Source {
            workspace,
            pane: 1,
            index: 0,
            tab: Tab::Docker,
        }
    }

    #[test]
    fn stamps_current_revision_after_focus_without_accepting_an_unstarted_drag() {
        let workspace = Uuid::new_v4();
        let source = source(workspace);
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        assert!(layout.focus(1));
        assert!(!source.accepts(None, workspace, 12, &layout));
        let stamp = source.stamp(workspace, 12, &layout);
        assert_eq!(stamp, Some(12));
        assert!(source.accepts(stamp, workspace, 12, &layout));
        assert!(!source.accepts(stamp, Uuid::new_v4(), 12, &layout));
    }

    #[test]
    fn changed_revision_rejects_stale_drop_before_center_fallback() {
        let workspace = Uuid::new_v4();
        let source = source(workspace);
        let mut layout = Layout::default();
        let before = layout.clone();
        let stamp = source.stamp(workspace, 1, &layout);
        // A rename/save or another window changes the shared revision while
        // retaining this tab's position. A missing edge preview cannot turn
        // that stale drag into a center reorder.
        if source.accepts(stamp, workspace, 2, &layout) {
            layout.drop_tab(source.tab, 1, Drop::Tab(4));
        }
        assert_eq!(layout, before);
        assert!(!source.accepts(stamp, workspace, 2, &layout));
    }

    #[test]
    fn moved_or_reordered_source_cannot_be_stamped_or_accepted() {
        let workspace = Uuid::new_v4();
        let source = source(workspace);
        for drop in [Drop::Right, Drop::Tab(4)] {
            let mut layout = Layout::default();
            let stamp = source.stamp(workspace, 1, &layout);
            assert!(layout.drop_tab(Tab::Docker, 1, drop));
            assert_eq!(source.stamp(workspace, 2, &layout), None);
            // Source identity is checked independently of the revision.
            assert!(!source.accepts(stamp, workspace, 1, &layout));
        }
    }

    #[test]
    fn valid_drag_keeps_center_and_all_split_edges_available() {
        let workspace = Uuid::new_v4();
        let source = source(workspace);
        for drop in [
            Drop::Tab(4),
            Drop::Left,
            Drop::Right,
            Drop::Top,
            Drop::Bottom,
        ] {
            let mut layout = Layout::default();
            let stamp = source.stamp(workspace, 7, &layout);
            assert!(source.accepts(stamp, workspace, 7, &layout));
            assert!(layout.drop_tab(source.tab, 1, drop));
            assert!(layout.validate().is_ok());
        }
    }
}
