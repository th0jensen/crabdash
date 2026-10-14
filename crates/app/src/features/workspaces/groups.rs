//! Dashboard tab-group targets, scoped commands and add-service picker state.
use super::{
    model,
    model::{Layout, Tab},
};
use crate::app::{Crabdash, MainTab};
use gpui::{Context, Window};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) workspace: Uuid,
    pub(crate) pane: u32,
}
impl Target {
    pub(crate) fn current(app: &Crabdash, pane: u32) -> Self {
        Self {
            workspace: app.workspaces.store.active,
            pane,
        }
    }
    pub(crate) fn accepts(self, workspace: Uuid, layout: &Layout) -> bool {
        self.workspace == workspace && layout.pane(self.pane).is_some()
    }
    pub(crate) fn validate(self, app: &mut Crabdash, cx: &mut Context<Crabdash>) -> bool {
        app.sync_workspace_store(cx);
        self.accepts(app.workspaces.store.active, app.workspaces.layout())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Picker {
    pub(crate) target: Target,
    revision: u64,
}
impl Picker {
    pub(crate) fn accepts(self, workspace: Uuid, revision: u64, layout: &Layout) -> bool {
        self.revision == revision && self.target.accepts(workspace, layout)
    }
}
impl Crabdash {
    pub(crate) fn toggle_dashboard_picker(
        &mut self,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !target.validate(self, cx) {
            return;
        }
        if self
            .workspaces
            .group_picker
            .is_some_and(|picker| picker.target == target)
        {
            self.workspaces.group_picker = None;
        } else {
            self.workspaces.group_picker = Some(Picker {
                target,
                revision: self.workspaces.revision(),
            });
        }
        self.focus_handle.focus(window);
        cx.notify();
    }
    pub(crate) fn add_dashboard_feature(
        &mut self,
        picker: Picker,
        tab: Tab,
        cx: &mut Context<Self>,
    ) {
        self.sync_workspace_store(cx);
        self.workspaces.group_picker = None;
        if !picker.accepts(
            self.workspaces.store.active,
            self.workspaces.revision(),
            self.workspaces.layout(),
        ) || !self
            .workspaces
            .layout()
            .available_features(picker.target.pane)
            .contains(&tab)
        {
            cx.notify();
            return;
        }
        self.select_pane_tab(picker.target.pane, tab.into(), cx);
    }
    pub(crate) fn select_workspace_tab(&mut self, tab: MainTab, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        let pane = self.workspaces.layout().focused;
        self.select_pane_tab(pane, tab, cx);
    }

    pub(crate) fn select_pane_tab(&mut self, pane: u32, tab: MainTab, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        // Selecting an already active tab should reveal it after manual scrolling.
        let sequence = self
            .workspaces
            .tab_reveal
            .map_or(1, |(_, _, sequence)| sequence.wrapping_add(1));
        self.workspaces.group_picker = None;
        self.workspaces.tab_reveal = Some((pane, tab.into(), sequence));
        if self
            .workspaces
            .layout_mut()
            .show_feature_in(pane, tab.into())
        {
            self.active_tab = tab;
            self.persist_workspace(cx);
        } else {
            cx.notify();
        }
    }

    pub(crate) fn close_workspace_tab(&mut self, pane: u32, tab: MainTab, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        if !self
            .workspaces
            .layout()
            .pane(pane)
            .is_some_and(|(tabs, _)| tabs.contains(&tab.into()))
        {
            return;
        }
        self.workspaces.group_picker = None;
        self.workspaces.drag_target = None;
        self.workspaces.resizing_split = None;
        self.workspaces.split_resize_grab = None;
        if self
            .workspaces
            .layout_mut()
            .close_feature_in(pane, tab.into())
        {
            self.active_tab = self.workspaces.layout().active().into();
            self.workspaces.tab_reveal = None;
            self.persist_workspace(cx);
        }
    }

    pub(crate) fn focus_workspace_pane(&mut self, pane: u32, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        if self.workspaces.layout_mut().focus(pane) {
            self.workspaces.group_picker = None;
            self.active_tab = self.workspaces.layout().active().into();
            self.persist_workspace(cx);
        }
    }

    pub(crate) fn drop_workspace_tab(
        &mut self,
        source: u32,
        tab: MainTab,
        target: u32,
        placement: model::Drop,
        cx: &mut Context<Self>,
    ) {
        self.sync_workspace_store(cx);
        self.workspaces.group_picker = None;
        self.workspaces.drag_target = None;
        self.workspaces.resizing_split = None;
        self.workspaces.split_resize_grab = None;
        if self
            .workspaces
            .layout_mut()
            .drop_from(source, tab.into(), target, placement)
        {
            self.active_tab = self.workspaces.layout().active().into();
            self.persist_workspace(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::workspaces::model::Drop;

    #[test]
    fn picker_is_scoped_to_workspace_pane_and_revision() {
        let workspace = Uuid::new_v4();
        let mut layout = Layout::default();
        assert!(layout.drop_tab(Tab::Services, 1, Drop::Right));
        let pane = layout.focused;
        let picker = Picker {
            target: Target { workspace, pane },
            revision: 7,
        };
        assert!(picker.accepts(workspace, 7, &layout));
        assert!(!picker.accepts(Uuid::new_v4(), 7, &layout));
        assert!(!picker.accepts(workspace, 8, &layout));
        assert!(layout.close_feature_in(pane, Tab::Services));
        assert!(!picker.accepts(workspace, 7, &layout));
    }
}
