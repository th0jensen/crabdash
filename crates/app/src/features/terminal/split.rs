//! Splitter drags stay within the current drawer and preserve grab offsets.
use super::panes;
use crate::{
    app::Crabdash,
    components::style,
    layout::{Axis, geometry::drag_ratio},
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};
use uuid::Uuid;

#[derive(Clone)]
struct Resize {
    owner: EntityId,
    workspace: Uuid,
    machine: Uuid,
    scope: Uuid,
    revision: u64,
    id: u32,
    axis: Axis,
    first_min: f32,
    second_min: f32,
    grab: Rc<Cell<Option<f32>>>,
}

impl Render for Resize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_0()
    }
}

impl Resize {
    fn apply(
        &self,
        app: &mut Crabdash,
        bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Crabdash>,
    ) {
        if self.owner != cx.entity_id()
            || self.workspace != app.workspaces.store.active
            || self.machine != app.selected_machine().uuid
            || !app.quake_terminal_open
            || app.preferences_open
            || app.machine_rename.target.is_some()
            || app.add_machine_modal_open
            || app.docker_run_modal_open
            || app.docker_removal.is_some()
            || app.workspaces.open
            || app.open_menu.is_some()
        {
            return;
        }
        let Some(drawer) = app.quake_terminals.get_mut(&self.machine) else {
            return;
        };
        if drawer.model.scope != self.scope || drawer.model.revision != self.revision {
            return;
        }
        let Some(grab) = self.grab.get() else {
            return;
        };
        let horizontal = self.axis == Axis::Horizontal;
        let (pointer, origin, extent) = if horizontal {
            (position.x, bounds.origin.x, bounds.size.width)
        } else {
            (position.y, bounds.origin.y, bounds.size.height)
        };
        if let Some(ratio) = drag_ratio(
            f32::from(pointer),
            f32::from(origin),
            f32::from(extent),
            grab,
            self.first_min,
            self.second_min,
        ) && drawer.model.layout.set_ratio(self.id, ratio)
        {
            drawer.model.drag_target = None;
            app.resize_quake_terminal(window, cx);
            cx.notify();
        }
    }
}

pub(super) fn render(
    app: &Crabdash,
    node: &crate::layout::Node<Uuid>,
    width: Pixels,
    height: Pixels,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
    render_child: fn(
        &Crabdash,
        &crate::layout::Node<Uuid>,
        Pixels,
        Pixels,
        &mut Window,
        &mut Context<Crabdash>,
    ) -> Stateful<Div>,
) -> Stateful<Div> {
    let crate::layout::Node::Split {
        id,
        axis,
        ratio,
        first,
        second,
    } = node
    else {
        return div().id("invalid-terminal-split");
    };
    let scale = f32::from(window.rem_size()) / 16.0;
    let metrics = panes::Metrics::new(scale, super::cell_metrics(&app.preferences, cx).1);
    let horizontal = *axis == Axis::Horizontal;
    let (a, b) = panes::extents(
        *axis,
        *ratio,
        first,
        second,
        f32::from(width),
        f32::from(height),
        metrics,
    );
    let (aw, ah, bw, bh) = if horizontal {
        (px(a), height, px(b), height)
    } else {
        (width, px(a), width, px(b))
    };
    let first_panel = render_child(app, first, aw, ah, window, cx);
    let second_panel = render_child(app, second, bw, bh, window, cx);
    let machine = app.selected_machine().uuid;
    let (scope, revision) = app
        .quake_terminals
        .get(&machine)
        .map_or((Uuid::nil(), 0), |drawer| {
            (drawer.model.scope, drawer.model.revision)
        });
    let bounds = Rc::new(Cell::new(None));
    let measure_bounds = bounds.clone();
    let measure = canvas(
        move |bounds, _, _| measure_bounds.set(Some(bounds)),
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0();
    let anchor = window.use_keyed_state(
        SharedString::from(format!("terminal-split-grab-{scope}-{revision}-{id}")),
        cx,
        |_, _| Rc::new(Cell::new(None)),
    );
    let grab = anchor.read(cx).clone();
    let mouse_grab = grab.clone();
    let mouse_bounds = bounds.clone();
    let owner = cx.entity().downgrade();
    let drag = Resize {
        owner: cx.entity_id(),
        workspace: app.workspaces.store.active,
        machine,
        scope,
        revision,
        id: *id,
        axis: *axis,
        first_min: panes::minimum(first, *axis, metrics),
        second_min: panes::minimum(second, *axis, metrics),
        grab,
    };
    let divider = crate::components::split_handle::render(
        SharedString::from(format!("terminal-divider-{machine}-{id}")),
        *axis,
        px(a),
    )
    .on_mouse_down(MouseButton::Left, move |event, _, cx| {
        mouse_grab.set(mouse_bounds.get().map(|bounds| {
            let (pointer, origin) = if horizontal {
                (event.position.x, bounds.origin.x)
            } else {
                (event.position.y, bounds.origin.y)
            };
            f32::from(pointer - origin) - a - 0.5
        }));
        cx.stop_propagation();
    })
    .on_drag(drag, move |drag, _, window, cx| {
        if let Some(bounds) = bounds.get() {
            owner
                .update(cx, |app, cx| {
                    drag.apply(app, bounds, window.mouse_position(), window, cx)
                })
                .ok();
        }
        cx.new(|_| drag.clone())
    });
    let id = *id;
    div()
        .id(SharedString::from(format!("terminal-split-{machine}-{id}")))
        .w(width)
        .h(height)
        .flex_none()
        .min_w_0()
        .min_h_0()
        .relative()
        .flex()
        .when(!horizontal, |this| this.flex_col())
        .on_drag_move(
            cx.listener(move |app, event: &DragMoveEvent<Resize>, window, cx| {
                let drag = event.drag(cx).clone();
                if drag.id == id {
                    drag.apply(app, event.bounds, event.event.position, window, cx);
                }
            }),
        )
        .child(first_panel)
        .child(
            div()
                .relative()
                .flex_none()
                .bg(rgb(style::BORDER))
                .when(horizontal, |this| this.w(px(1.0)).h_full())
                .when(!horizontal, |this| this.h(px(1.0)).w_full()),
        )
        .child(second_panel)
        .child(divider)
        .child(measure)
}
