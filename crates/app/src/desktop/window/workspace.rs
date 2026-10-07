//! Native pane windows share their controller with the main workspace.
use crate::{Crabdash, features::workspaces::model::WindowRect};
use gpui::{prelude::*, *};
use uuid::Uuid;

pub(crate) struct Quitting;
impl Global for Quitting {}

pub(crate) struct DetachedWorkspace {
    owner: Entity<Crabdash>,
    workspace: Uuid,
    id: u32,
    window_id: WindowId,
    _changes: Subscription,
    _bounds: Subscription,
}

impl DetachedWorkspace {
    pub(crate) fn owner(&self) -> Entity<Crabdash> {
        self.owner.clone()
    }

    fn new(
        owner: Entity<Crabdash>,
        workspace: Uuid,
        id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let changes = cx.observe(&owner, |_, _, cx| cx.notify());
        let bounds = cx.observe_window_bounds(window, {
            let owner = owner.clone();
            move |_, window, cx| {
                let bounds = window.bounds();
                owner.update(cx, |app, cx| {
                    app.sync_workspace_store(cx);
                    if app.workspaces.store.active == workspace {
                        app.workspaces.layout_mut().set_bounds(
                            id,
                            WindowRect {
                                x: f32::from(bounds.origin.x),
                                y: f32::from(bounds.origin.y),
                                width: f32::from(bounds.size.width),
                                height: f32::from(bounds.size.height),
                            },
                        );
                        app.persist_workspace(cx);
                    }
                });
            }
        });
        cx.on_release(|pane, cx| {
            if cx.try_global::<Quitting>().is_some() {
                return;
            }
            pane.owner.update(cx, |app, cx| {
                app.sync_workspace_store(cx);
                app.detached_windows.remove(&(pane.workspace, pane.id));
                if app.terminal_window == Some(pane.window_id) {
                    app.terminal_window = None;
                }
                if app.overlay_window == Some(pane.window_id) {
                    app.preferences_open = false;
                    app.add_machine_modal_open = false;
                    app.docker_run_modal_open = false;
                    app.docker_removal = None;
                    app.workspaces.open = false;
                    app.open_menu = None;
                    app.overlay_window = None;
                }
                cx.notify();
                if app.workspaces.store.active == pane.workspace
                    && app.workspaces.layout_mut().move_back(pane.id)
                {
                    app.persist_workspace(cx);
                }
            });
        })
        .detach();
        Self {
            owner,
            workspace,
            id,
            window_id: window.window_handle().window_id(),
            _changes: changes,
            _bounds: bounds,
        }
    }
}

impl Render for DetachedWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.owner.update(cx, |app, cx| {
            app.render_workspace(Some(self.id), window, cx)
        })
    }
}

pub(crate) fn open_detached_workspace_window(
    owner: Entity<Crabdash>,
    workspace: Uuid,
    id: u32,
    cx: &mut App,
) -> anyhow::Result<()> {
    // Docking callbacks already borrow the controller; create the native window
    // after that borrow ends, and recheck the layout before opening.
    cx.defer(move |cx| {
        let pane = owner.update(cx, |app, cx| {
            app.sync_workspace_store(cx);
            if app.workspaces.store.active != workspace
                || app.detached_windows.contains_key(&(workspace, id))
            {
                return None;
            }
            app.workspaces
                .layout()
                .detached
                .iter()
                .find(|pane| pane.id == id)
                .cloned()
        });
        let Some(pane) = pane else {
            return;
        };
        let mut options = super::options(cx, false);
        options.window_min_size = Some(size(px(380.0), px(300.0)));
        options.window_bounds = Some(match pane.bounds {
            Some(bounds) => WindowBounds::Windowed(Bounds::new(
                point(px(bounds.x), px(bounds.y)),
                size(px(bounds.width), px(bounds.height)),
            )),
            None => WindowBounds::centered(size(px(660.0), px(480.0)), cx),
        });
        let parent = owner.clone();
        let result = cx.open_window(options, move |window, cx| {
            super::prepare(window, cx);
            // Closing a pane returns its tabs; it does not hide them in the tray.
            window.on_window_should_close(cx, |_, _| true);
            cx.new(|cx| DetachedWorkspace::new(parent, workspace, id, window, cx))
        });
        owner.update(cx, |app, cx| {
            match result {
                Ok(handle) => {
                    app.detached_windows.insert((workspace, id), handle);
                }
                Err(error) => {
                    app.workspaces.layout_mut().move_back(id);
                    app.workspaces.error = Some(format!("Could not open the pane window: {error}"));
                    app.persist_workspace(cx);
                }
            }
            cx.notify();
        });
    });
    Ok(())
}

pub(crate) fn schedule_workspace_windows(app: &mut Crabdash, cx: &mut Context<Crabdash>) {
    if app.detached_reconcile_scheduled {
        return;
    }
    app.detached_reconcile_scheduled = true;
    let owner = cx.entity();
    cx.defer(move |cx| {
        owner.update(cx, |app, cx| {
            app.detached_reconcile_scheduled = false;
            app.sync_workspace_store(cx);
            let active = app.workspaces.store.active;
            let desired = app.workspaces.layout().detached.clone();
            let obsolete: Vec<_> = app
                .detached_windows
                .iter()
                .filter(|((workspace, id), _)| {
                    *workspace != active || !desired.iter().any(|pane| pane.id == *id)
                })
                .map(|(key, handle)| (*key, *handle))
                .collect();
            for (key, handle) in obsolete {
                app.detached_windows.remove(&key);
                if let Err(error) = handle.update(cx, |_, window, _| window.remove_window()) {
                    tracing::debug!(%error, "Pane window already closed");
                }
            }
            for pane in desired {
                if !app.detached_windows.contains_key(&(active, pane.id)) {
                    if let Err(error) =
                        open_detached_workspace_window(cx.entity(), active, pane.id, cx)
                    {
                        app.workspaces.error = Some(error.to_string());
                    }
                }
            }
        });
    });
}

pub(crate) fn close_detached(window: &mut Window) -> bool {
    if window
        .window_handle()
        .downcast::<DetachedWorkspace>()
        .is_some()
    {
        window.remove_window();
        true
    } else {
        false
    }
}
