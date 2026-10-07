//! In-window dashboard panes, saved workspaces, and their compact switcher.
pub(crate) mod model;
mod store;
mod view;
pub(crate) use view::{button, popup};

use crate::app::{Crabdash, MainTab};
use crate::components::text_field::TextField;
use gpui::{AppContext, Context, Entity, Global, Subscription, Window};
use model::{Layout, Tab};
use store::{Store, validate_name};
use uuid::Uuid;

impl From<MainTab> for Tab {
    fn from(tab: MainTab) -> Self {
        match tab {
            MainTab::Docker => Self::Docker,
            MainTab::Disks => Self::Disks,
            MainTab::Services => Self::Services,
            MainTab::System => Self::System,
        }
    }
}
impl From<Tab> for MainTab {
    fn from(tab: Tab) -> Self {
        match tab {
            Tab::Docker => Self::Docker,
            Tab::Disks => Self::Disks,
            Tab::Services => Self::Services,
            Tab::System => Self::System,
        }
    }
}

#[derive(Clone)]
struct SharedStore {
    store: Store,
    revision: u64,
    read_only: bool,
    error: Option<String>,
}
impl Global for SharedStore {}

pub(crate) struct State {
    pub store: Store,
    pub open: bool,
    pub rename: Option<Uuid>,
    pub drag_target: Option<(u32, model::Drop)>,
    pub resizing_split: Option<u32>,
    pub name: Entity<TextField>,
    pub error: Option<String>,
    pub read_only: bool,
    apply_runtime: bool,
    revision: u64,
    _changes: Subscription,
}

impl State {
    pub(crate) fn load(cx: &mut Context<Crabdash>) -> Self {
        if cx.try_global::<SharedStore>().is_none() {
            let (store, error, read_only) = match Store::load(
                crate::features::preferences::current(cx).sidebar_width,
            ) {
                Ok(store) => (store, None, false),
                Err(error) => (
                    Store::default(),
                    Some(format!(
                        "{} Saved layouts have been preserved. Use Reset saved layouts to recover.",
                        error
                    )),
                    true,
                ),
            };
            cx.set_global(SharedStore {
                store,
                revision: 0,
                read_only,
                error,
            });
        }
        let shared = cx.global::<SharedStore>().clone();
        let changes = cx.observe_global::<SharedStore>(|app, cx| {
            app.sync_workspace_store(cx);
            cx.notify();
        });
        Self {
            store: shared.store,
            open: false,
            rename: None,
            drag_target: None,
            resizing_split: None,
            name: cx.new(|cx| TextField::new("Workspace name", "Workspace name", 1, cx).compact()),
            error: shared.error,
            read_only: shared.read_only,
            apply_runtime: true,
            revision: shared.revision,
            _changes: changes,
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn layout(&self) -> &Layout {
        &self.store.current().layout
    }
    pub(crate) fn layout_mut(&mut self) -> &mut Layout {
        &mut self.store.current_mut().layout
    }
}

impl Crabdash {
    pub(crate) fn sync_workspace_store(&mut self, cx: &mut Context<Self>) {
        let shared = cx.global::<SharedStore>();
        if self.workspaces.revision == shared.revision {
            return;
        }
        self.workspaces.apply_runtime = true;
        self.workspaces.drag_target = None;
        self.workspaces.resizing_split = None;
        self.workspaces.store = shared.store.clone();
        self.workspaces.revision = shared.revision;
        self.workspaces.read_only = shared.read_only;
        self.workspaces.error = shared.error.clone();
        let workspace = self.workspaces.store.current();
        self.sidebar_collapsed = workspace.sidebar_collapsed;
        self.sidebar_width = gpui::px(workspace.sidebar_width);
        self.active_tab = workspace.layout.active().into();
    }

    pub(crate) fn apply_workspace_runtime(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.workspaces.apply_runtime {
            return;
        }
        self.workspaces.apply_runtime = false;
        let workspace = self.workspaces.store.current();
        let desired = workspace.terminal_open;
        self.sidebar_collapsed = workspace.sidebar_collapsed;
        self.sidebar_width = gpui::px(workspace.sidebar_width);
        self.active_tab = workspace.layout.active().into();
        if desired != self.quake_terminal_open {
            self.set_quake_terminal_open(desired, window, cx);
        }
    }

    pub(crate) fn recover_workspaces(&mut self, cx: &mut Context<Self>) {
        self.workspaces.store = Store::default();
        self.workspaces.read_only = false;
        self.workspaces.apply_runtime = true;
        self.workspaces.error = None;
        self.workspaces.revision = cx.global::<SharedStore>().revision;
        self.persist_workspace(cx);
    }

    pub(crate) fn persist_workspace(&mut self, cx: &mut Context<Self>) {
        if self.workspaces.read_only {
            cx.notify();
            return;
        }
        if self.workspaces.revision != cx.global::<SharedStore>().revision {
            self.sync_workspace_store(cx);
            self.workspaces.error = Some(
                "Another window updated the saved layouts. Reopen Workspaces and try again.".into(),
            );
            cx.notify();
            return;
        }
        let workspace = self.workspaces.store.current_mut();
        workspace.sidebar_collapsed = self.sidebar_collapsed;
        workspace.sidebar_width = f32::from(self.sidebar_width).clamp(180.0, 420.0);
        if !self.workspaces.apply_runtime {
            workspace.terminal_open = self.quake_terminal_open;
        }
        if let Err(error) = self.workspaces.store.save() {
            self.workspaces.error = Some(error.to_string());
        } else {
            self.workspaces.revision = self.workspaces.revision.saturating_add(1);
            cx.set_global(SharedStore {
                store: self.workspaces.store.clone(),
                revision: self.workspaces.revision,
                read_only: false,
                error: self.workspaces.error.clone(),
            });
        }
        cx.notify();
    }

    pub(crate) fn select_workspace_tab(&mut self, tab: MainTab, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        if self.workspaces.layout_mut().show(tab.into()) {
            self.active_tab = tab;
            self.refresh_services(cx);
            self.persist_workspace(cx);
        }
    }

    pub(crate) fn select_pane_tab(&mut self, pane: u32, tab: MainTab, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        if self
            .workspaces
            .layout()
            .root
            .pane(pane)
            .is_some_and(|(tabs, _)| tabs.contains(&tab.into()))
        {
            self.select_workspace_tab(tab, cx);
        }
    }

    pub(crate) fn focus_workspace_pane(&mut self, pane: u32, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        if self.workspaces.layout_mut().focus(pane) {
            self.active_tab = self.workspaces.layout().active().into();
            self.persist_workspace(cx);
        }
    }

    pub(crate) fn drop_workspace_tab(
        &mut self,
        tab: MainTab,
        target: u32,
        placement: model::Drop,
        cx: &mut Context<Self>,
    ) {
        self.sync_workspace_store(cx);
        self.workspaces.drag_target = None;
        self.workspaces.resizing_split = None;
        if self
            .workspaces
            .layout_mut()
            .drop_tab(tab.into(), target, placement)
        {
            self.active_tab = self.workspaces.layout().active().into();
            self.refresh_services(cx);
            self.persist_workspace(cx);
        }
    }

    pub(crate) fn restore_workspace(
        &mut self,
        id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_workspace_store(cx);
        if !self
            .workspaces
            .store
            .workspaces
            .iter()
            .any(|workspace| workspace.id == id)
        {
            return;
        }
        self.persist_workspace(cx);
        self.workspaces.drag_target = None;
        self.workspaces.resizing_split = None;
        self.workspaces.store.active = id;
        let workspace = self.workspaces.store.current();
        self.sidebar_collapsed = workspace.sidebar_collapsed;
        self.sidebar_width = gpui::px(workspace.sidebar_width);
        let terminal_open = workspace.terminal_open;
        self.workspaces.apply_runtime = false;
        self.active_tab = workspace.layout.active().into();
        self.workspaces.open = false;
        self.workspaces.rename = None;
        if self.quake_terminal_open != terminal_open {
            self.set_quake_terminal_open(terminal_open, window, cx);
        }
        self.refresh_services(cx);
        self.persist_workspace(cx);
        self.focus_handle.focus(window);
    }

    pub(crate) fn save_workspace_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        if self.workspaces.store.workspaces.len() >= 24 {
            self.workspaces.error = Some("You can save up to 24 workspaces.".into());
            cx.notify();
            return;
        }
        self.persist_workspace(cx);
        let mut workspace = self.workspaces.store.current().clone();
        workspace.id = Uuid::new_v4();
        workspace.name = format!("Workspace {}", self.workspaces.store.workspaces.len() + 1);
        self.workspaces.store.active = workspace.id;
        let id = workspace.id;
        self.workspaces.store.workspaces.push(workspace);
        self.persist_workspace(cx);
        self.rename_workspace(id, window, cx);
    }

    pub(crate) fn rename_workspace(
        &mut self,
        id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_workspace_store(cx);
        let Some(workspace) = self
            .workspaces
            .store
            .workspaces
            .iter()
            .find(|workspace| workspace.id == id)
        else {
            return;
        };
        let name = workspace.name.clone();
        self.workspaces
            .name
            .update(cx, |field, cx| field.set_text(&name, cx));
        self.workspaces.rename = Some(id);
        self.workspaces.error = None;
        window.focus(&gpui::Focusable::focus_handle(
            self.workspaces.name.read(cx),
            cx,
        ));
        cx.notify();
    }

    pub(crate) fn finish_workspace_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        let name = self.workspaces.name.read(cx).text().trim().to_string();
        if let Err(error) = validate_name(&name) {
            self.workspaces.error = Some(error.to_string());
            cx.notify();
            return;
        }
        if let Some(id) = self.workspaces.rename.take() {
            if let Some(workspace) = self
                .workspaces
                .store
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == id)
            {
                workspace.name = name;
            }
        }
        self.workspaces.error = None;
        self.persist_workspace(cx);
        self.focus_handle.focus(window);
    }

    pub(crate) fn remove_workspace(
        &mut self,
        id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_workspace_store(cx);
        if self.workspaces.store.workspaces.len() <= 1 {
            return;
        }
        if self.workspaces.store.active == id {
            if let Some(next) = self
                .workspaces
                .store
                .workspaces
                .iter()
                .find(|workspace| workspace.id != id)
                .map(|workspace| workspace.id)
            {
                self.restore_workspace(next, window, cx);
            }
        }
        self.workspaces
            .store
            .workspaces
            .retain(|workspace| workspace.id != id);
        self.workspaces.rename = None;
        self.persist_workspace(cx);
    }
}
