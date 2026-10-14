//! A pane owns its add-tab action independently of keyboard focus.
use super::model::PaneTarget;
use crate::app::Crabdash;
use gpui::EntityId;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(super) struct GroupTarget {
    owner: EntityId,
    workspace: Uuid,
    machine: Uuid,
    pub pane: PaneTarget,
}

impl GroupTarget {
    pub fn new(app: &Crabdash, pane: u32, owner: EntityId) -> Option<Self> {
        let machine = app.selected_machine().uuid;
        Some(Self {
            owner,
            workspace: app.workspaces.store.active,
            machine,
            pane: app.quake_terminals.get(&machine)?.model.group(pane)?,
        })
    }

    pub fn valid(self, app: &Crabdash, owner: EntityId) -> bool {
        self.owner == owner
            && self.workspace == app.workspaces.store.active
            && self.machine == app.selected_machine().uuid
            && app.quake_terminal_open
            && !app.preferences_open
            && !app.add_machine_modal_open
            && !app.workspaces.open
            && app.machine_rename.target.is_none()
            && !app.docker_run_modal_open
            && app.docker_removal.is_none()
            && app.open_menu.is_none()
            && app
                .quake_terminals
                .get(&self.machine)
                .is_some_and(|drawer| drawer.model.accepts_group(self.pane))
    }
}
