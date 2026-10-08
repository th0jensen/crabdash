//! Measured glyph hit-testing and guarded held-pointer lifetime.
use crate::app::Crabdash;
use gpui::{prelude::*, *};
use libghostty_vt::{
    selection::gesture::{Autoscroll, Geometry},
    terminal::PointCoordinate,
};
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Grid {
    pub(super) origin: Point<Pixels>,
    cell_width: f32,
    cell_height: f32,
    height: f32,
    columns: u16,
    rows: u16,
}

impl Grid {
    pub(super) fn new(
        bounds: Bounds<Pixels>,
        columns: u16,
        rows: u16,
        cell_width: f32,
        cell_height: f32,
    ) -> Option<Self> {
        let height = (f32::from(bounds.size.height)
            - super::super::geometry::CONTENT_TOP
            - super::super::geometry::CONTENT_BOTTOM)
            .min(f32::from(rows) * cell_height);
        (columns > 0
            && rows > 0
            && cell_width.is_finite()
            && cell_width > 0.0
            && cell_height.is_finite()
            && cell_height > 0.0
            && height > 0.0)
            .then_some(Self {
                origin: bounds.origin
                    + point(
                        px(super::super::geometry::CONTENT_SIDE),
                        px(super::super::geometry::CONTENT_TOP),
                    ),
                cell_width,
                cell_height,
                height,
                columns,
                rows,
            })
    }

    pub(super) fn bounds(self) -> Bounds<Pixels> {
        Bounds::new(
            self.origin,
            size(
                px(f32::from(self.columns) * self.cell_width),
                px(self.height),
            ),
        )
    }

    pub(super) fn cell(self, position: Point<Pixels>) -> PointCoordinate {
        let local = position - self.origin;
        PointCoordinate {
            x: (f32::from(local.x) / self.cell_width)
                .floor()
                .clamp(0.0, f32::from(self.columns - 1)) as u16,
            y: (f32::from(local.y) / self.cell_height)
                .floor()
                .clamp(0.0, f32::from(self.rows - 1)) as u32,
        }
    }

    pub(super) fn surface(self, position: Point<Pixels>) -> (f64, f64, Geometry) {
        // Normalize fractional cell positions to exact integer geometry units.
        // Rounding the font advance itself accumulates drift near the right edge.
        let local = position - self.origin;
        let height = self.height.ceil().max(1.0) as u32;
        (
            f64::from(f32::from(local.x) / self.cell_width) * 100.0,
            f64::from(f32::from(local.y) / self.height) * f64::from(height),
            Geometry {
                columns: u32::from(self.columns),
                cell_width: 100,
                padding_left: 0,
                screen_height: height,
            },
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Target {
    owner: EntityId,
    workspace: Uuid,
    workspace_revision: u64,
    machine: Uuid,
    session: Uuid,
    input: EntityId,
    scope: Uuid,
    revision: u64,
}

impl Target {
    fn valid(self, app: &Crabdash, window: &Window, cx: &App) -> bool {
        super::super::actions::allowed(app, cx)
            && app.open_menu.is_none()
            && window.is_window_active()
            && app.quake_terminal_open
            && self.workspace == app.workspaces.store.active
            && self.workspace_revision == app.workspaces.revision()
            && self.machine == app.selected_machine().uuid
            && window
                .root::<Crabdash>()
                .flatten()
                .is_some_and(|root| root.entity_id() == self.owner)
            && app
                .quake_terminals
                .get(&self.machine)
                .is_some_and(|drawer| {
                    drawer.model.scope == self.scope
                        && drawer.model.revision == self.revision
                        && drawer.model.layout.focused_tab() == Some(self.session)
                        && drawer.sessions.get(&self.session).is_some_and(|session| {
                            session.input.entity_id() == self.input
                                && session.rename.is_none()
                                && session.input.focus_handle(cx).is_focused(window)
                        })
                })
    }

    fn held_valid(self, app: &Crabdash, window: &Window, cx: &App) -> bool {
        // Windows publishes hover(false) without a MouseExitEvent.
        #[cfg(not(target_os = "macos"))]
        if !window.is_window_hovered() {
            return false;
        }
        self.valid(app, window, cx)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Held {
    id: Uuid,
    target: Target,
    grid: Grid,
    position: Point<Pixels>,
    rectangle: bool,
}

fn fail(app: &mut Crabdash, target: Target, error: anyhow::Error, cx: &mut Context<Crabdash>) {
    if let Some(session) = app
        .quake_terminals
        .get_mut(&target.machine)
        .and_then(|drawer| drawer.sessions.get_mut(&target.session))
    {
        session.terminal.cancel_selection_gesture();
    }
    app.set_status_error(format!("Unable to select terminal text: {error}"));
    cx.notify();
}

/// Canvas listeners own only a held text gesture; other panes and controls keep their events.
pub(crate) fn layer(
    app: &Crabdash,
    machine: Uuid,
    session: Uuid,
    window: &Window,
    cx: &Context<Crabdash>,
) -> impl IntoElement {
    let drawer = app.quake_terminals.get(&machine);
    let terminal = drawer.and_then(|drawer| drawer.sessions.get(&session));
    let target = drawer.zip(terminal).map(|(drawer, session_state)| Target {
        owner: cx.entity_id(),
        workspace: app.workspaces.store.active,
        workspace_revision: app.workspaces.revision(),
        machine,
        session,
        input: session_state.input.entity_id(),
        scope: drawer.model.scope,
        revision: drawer.model.revision,
    });
    let geometry = terminal.map(|session| session.terminal.selection.geometry.clone());
    let columns = terminal.map_or(0, |session| session.size.columns);
    let rows = terminal.map_or(0, |session| session.size.rows);
    let (cell_width, cell_height) = super::super::cell_metrics(&app.preferences, cx);
    let owner = cx.entity().downgrade();
    let _ = window;
    canvas(
        move |bounds, window, _| {
            let grid = Grid::new(bounds, columns, rows, cell_width, cell_height);
            if let Some(geometry) = &geometry {
                geometry.set(grid);
            }
            grid.map(|grid| {
                (
                    grid,
                    window.insert_hitbox(grid.bounds(), HitboxBehavior::Normal),
                )
            })
        },
        move |_, frame, window, _| {
            let Some(target) = target else {
                return;
            };
            let Some((grid, hitbox)) = frame else {
                return;
            };
            let press_owner = owner.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture
                    || event.button != MouseButton::Left
                    || !hitbox.is_hovered(window)
                    || !grid.bounds().contains(&event.position)
                {
                    return;
                }
                press_owner
                    .update(cx, |app, cx| {
                        if !target.valid(app, window, cx) {
                            return;
                        }
                        let (x, y, _) = grid.surface(event.position);
                        let result = app
                            .quake_terminals
                            .get_mut(&target.machine)
                            .and_then(|drawer| drawer.sessions.get_mut(&target.session))
                            .map(|session| {
                                session.terminal.selection_press(
                                    grid.cell(event.position),
                                    x,
                                    y,
                                    event.click_count,
                                )?;
                                session.terminal.selection.held = Some(Held {
                                    id: Uuid::new_v4(),
                                    target,
                                    grid,
                                    position: event.position,
                                    rectangle: event.modifiers.alt,
                                });
                                Ok::<(), anyhow::Error>(())
                            });
                        if let Some(Err(error)) = result {
                            fail(app, target, error, cx);
                        }
                        cx.notify();
                        cx.stop_propagation();
                    })
                    .ok();
            });
            let move_owner = owner.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                move_owner
                    .update(cx, |app, cx| {
                        let held = app
                            .quake_terminals
                            .get(&target.machine)
                            .and_then(|drawer| drawer.sessions.get(&target.session))
                            .and_then(|session| session.terminal.selection.held);
                        if held.is_none_or(|held| held.target != target) {
                            return;
                        }
                        if event.pressed_button != Some(MouseButton::Left)
                            || !target.held_valid(app, window, cx)
                        {
                            app.cancel_terminal_selection(target);
                            return;
                        }
                        app.move_terminal_selection(
                            target,
                            event.position,
                            event.modifiers.alt,
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    })
                    .ok();
            });
            let up_owner = owner.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                    return;
                }
                up_owner
                    .update(cx, |app, cx| {
                        let held = app
                            .quake_terminals
                            .get(&target.machine)
                            .and_then(|drawer| drawer.sessions.get(&target.session))
                            .and_then(|session| session.terminal.selection.held);
                        let Some(held) = held.filter(|held| held.target == target) else {
                            return;
                        };
                        if !target.held_valid(app, window, cx) {
                            app.cancel_terminal_selection(target);
                            return;
                        }
                        if let Some(session) = app
                            .quake_terminals
                            .get_mut(&target.machine)
                            .and_then(|drawer| drawer.sessions.get_mut(&target.session))
                        {
                            if let Err(error) = session
                                .terminal
                                .selection_release(Some(held.grid.cell(event.position)))
                            {
                                fail(app, target, error, cx);
                            }
                        }
                        cx.notify();
                        cx.stop_propagation();
                    })
                    .ok();
            });
            let exit_owner = owner.clone();
            window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    exit_owner
                        .update(cx, |app, _| app.cancel_terminal_selection(target))
                        .ok();
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

impl Crabdash {
    fn cancel_terminal_selection(&mut self, target: Target) {
        if let Some(session) = self
            .quake_terminals
            .get_mut(&target.machine)
            .and_then(|drawer| drawer.sessions.get_mut(&target.session))
            .filter(|session| {
                session.input.entity_id() == target.input
                    && session
                        .terminal
                        .selection
                        .held
                        .is_some_and(|held| held.target == target)
            })
        {
            session.terminal.cancel_selection_gesture();
        }
    }

    fn move_terminal_selection(
        &mut self,
        target: Target,
        position: Point<Pixels>,
        rectangle: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self
            .quake_terminals
            .get_mut(&target.machine)
            .and_then(|drawer| drawer.sessions.get_mut(&target.session))
        else {
            return;
        };
        let Some(mut held) = session.terminal.selection.held else {
            return;
        };
        if held.target != target || session.terminal.selection.geometry.get() != Some(held.grid) {
            session.terminal.cancel_selection_gesture();
            return;
        }
        held.position = position;
        held.rectangle = rectangle;
        let (x, y, geometry) = held.grid.surface(position);
        if let Err(error) =
            session
                .terminal
                .selection_drag(held.grid.cell(position), x, y, rectangle, geometry)
        {
            fail(self, target, error, cx);
            return;
        }
        session.terminal.selection.held = Some(held);
        let scroll = session.terminal.selection_autoscroll() != Autoscroll::None;
        if !scroll {
            session.terminal.selection.autoscroll = None;
        }
        if scroll && session.terminal.selection.autoscroll.is_none() {
            session.terminal.selection.autoscroll = Some(cx.spawn_in(
                window,
                async move |this: WeakEntity<Crabdash>, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(50))
                            .await;
                        if !this
                            .update_in(cx, |app, window, cx| {
                                app.tick_terminal_selection(target, held.id, window, cx)
                            })
                            .unwrap_or(false)
                        {
                            break;
                        }
                    }
                },
            ));
        }
        cx.notify();
    }

    fn tick_terminal_selection(
        &mut self,
        target: Target,
        id: Uuid,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let owned = self
            .quake_terminals
            .get(&target.machine)
            .and_then(|drawer| drawer.sessions.get(&target.session))
            .is_some_and(|session| {
                session
                    .terminal
                    .selection
                    .held
                    .is_some_and(|held| held.target == target && held.id == id)
            });
        if !owned {
            return false;
        }
        if !target.held_valid(self, window, cx) {
            self.cancel_terminal_selection(target);
            return false;
        }
        let Some(session) = self
            .quake_terminals
            .get_mut(&target.machine)
            .and_then(|drawer| drawer.sessions.get_mut(&target.session))
        else {
            return false;
        };
        let Some(held) = session
            .terminal
            .selection
            .held
            .filter(|held| held.target == target && held.id == id)
        else {
            return false;
        };
        if session.terminal.selection.geometry.get() != Some(held.grid) {
            session.terminal.cancel_selection_gesture();
            return false;
        }
        if session.terminal.selection_autoscroll() == Autoscroll::None {
            session.terminal.selection.autoscroll = None;
            return false;
        }
        let (x, y, geometry) = held.grid.surface(held.position);
        if let Err(error) = session.terminal.selection_tick(
            held.grid.cell(held.position),
            x,
            y,
            held.rectangle,
            geometry,
        ) {
            fail(self, target, error, cx);
            return false;
        }
        cx.notify();
        true
    }

    pub(crate) fn reconcile_terminal_selection(&mut self, window: &Window, cx: &App) {
        let cancel = self
            .quake_terminals
            .values()
            .flat_map(|drawer| drawer.sessions.values())
            .filter_map(|session| {
                session.terminal.selection.held.filter(|held| {
                    !held.target.held_valid(self, window, cx)
                        || session.terminal.selection.geometry.get() != Some(held.grid)
                })
            })
            .map(|held| held.target)
            .collect::<Vec<_>>();
        for target in cancel {
            self.cancel_terminal_selection(target);
        }
    }
}
