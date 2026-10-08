//! Terminal drags have a distinct scope from dashboard feature drags.
use crate::{
    app::Crabdash,
    components::style,
    layout::{Drop, geometry::target},
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use std::{cell::Cell, rc::Rc};
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct DraggedSession {
    pub session: Uuid,
    pub pane: u32,
    pub index: usize,
    owner: EntityId,
    workspace: Uuid,
    machine: Uuid,
    scope: Uuid,
    revision: u64,
    title: String,
}

impl DraggedSession {
    pub fn new(
        app: &Crabdash,
        pane: u32,
        index: usize,
        session: Uuid,
        owner: EntityId,
        title: String,
    ) -> Self {
        let machine = app.selected_machine().uuid;
        let (scope, revision) = app
            .quake_terminals
            .get(&machine)
            .map_or((Uuid::nil(), 0), |drawer| {
                (drawer.model.scope, drawer.model.revision)
            });
        Self {
            session,
            pane,
            index,
            owner,
            workspace: app.workspaces.store.active,
            machine,
            scope,
            revision,
            title,
        }
    }

    pub fn valid(&self, app: &Crabdash, owner: EntityId) -> bool {
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
                .is_some_and(|drawer| {
                    drawer.model.accepts(
                        self.scope,
                        self.revision,
                        self.pane,
                        self.index,
                        self.session,
                    )
                })
    }
}

impl Render for DraggedSession {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if window
            .root::<Crabdash>()
            .flatten()
            .is_none_or(|view| view.entity_id() != self.owner)
        {
            return div();
        }
        crate::components::drag_preview::render(
            self.title.clone(),
            crate::components::common::lucide_icon(Icon::Terminal, style::ICON),
        )
    }
}

pub(super) fn body(
    app: &Crabdash,
    pane: u32,
    panel: Div,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let machine = app.selected_machine().uuid;
    let center = app
        .quake_terminals
        .get(&machine)
        .and_then(|drawer| drawer.model.layout.pane(pane))
        .and_then(|(tabs, active)| tabs.iter().position(|id| *id == active))
        .unwrap_or(0);
    let drop = app
        .quake_terminals
        .get(&machine)
        .and_then(|drawer| drawer.model.drag_target)
        .filter(|(id, _)| *id == pane)
        .map(|(_, drop)| drop);
    let bounds = Rc::new(Cell::new(None));
    let measure_bounds = bounds.clone();
    let measure = canvas(
        move |bounds, _, _| measure_bounds.set(Some(bounds)),
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0();
    let owner = cx.entity().downgrade();
    let overlay = div()
        .id(SharedString::from(format!("terminal-dock-overlay-{pane}")))
        .absolute()
        .inset_0()
        .opacity(0.0)
        .drag_over::<DraggedSession>(move |this, drag, _, cx| {
            if owner
                .update(cx, |app, cx| drag.valid(app, cx.entity_id()))
                .is_ok_and(|valid| valid)
            {
                this.opacity(1.0)
            } else {
                this
            }
        })
        .when(cx.has_active_drag() && drop.is_some(), |this| {
            this.occlude()
        })
        .child(
            div()
                .absolute()
                .bg(rgba(0x383B3D88))
                .border_1()
                .border_color(rgb(style::TAB_INDICATOR))
                .map(|this| match drop {
                    Some(Drop::Left) => this.left_0().top_0().bottom_0().w(relative(0.5)),
                    Some(Drop::Right) => this.right_0().top_0().bottom_0().w(relative(0.5)),
                    Some(Drop::Top) => this.left_0().right_0().top_0().h(relative(0.5)),
                    Some(Drop::Bottom) => this.left_0().right_0().bottom_0().h(relative(0.5)),
                    _ => this.inset_0(),
                }),
        )
        .on_drop(cx.listener(move |app, drag: &DraggedSession, window, cx| {
            if drag.valid(app, cx.entity_id())
                && let Some(drop) = bounds
                    .get()
                    .and_then(|bounds| target(bounds, window.mouse_position(), center))
            {
                app.drop_terminal_tab(machine, drag.session, pane, drop, window, cx);
            }
            cx.stop_propagation();
        }));
    div()
        .id(SharedString::from(format!("terminal-pane-body-{pane}")))
        .flex_1()
        .min_h_0()
        .min_w_0()
        .relative()
        .overflow_hidden()
        .on_drag_move(
            cx.listener(move |app, event: &DragMoveEvent<DraggedSession>, _, cx| {
                let drag = event.drag(cx);
                let candidate = drag
                    .valid(app, cx.entity_id())
                    .then(|| target(event.bounds, event.event.position, center))
                    .flatten()
                    .map(|drop| (pane, drop));
                if let Some(drawer) = app.quake_terminals.get_mut(&machine) {
                    if candidate.is_some()
                        || drawer.model.drag_target.is_some_and(|(id, _)| id == pane)
                    {
                        if drawer.model.drag_target != candidate {
                            drawer.model.drag_target = candidate;
                            cx.notify();
                        }
                    }
                }
            }),
        )
        .child(panel)
        .child(overlay)
        .child(measure)
}
