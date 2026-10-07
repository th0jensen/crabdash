//! Geometric in-window docking targets, matching Zed's edge-band behavior.
use super::header::{DraggedTab, valid_hover};
use crate::{app::Crabdash, components::style, features::workspaces::model::Drop};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};

fn target(bounds: Bounds<Pixels>, position: Point<Pixels>, center: usize) -> Option<Drop> {
    if !bounds.contains(&position) {
        return None;
    }
    let x = f32::from(position.x - bounds.origin.x);
    let y = f32::from(position.y - bounds.origin.y);
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let band = width.min(height) * 0.2;
    let edges = [
        (y, Drop::Top),
        (width - x, Drop::Right),
        (height - y, Drop::Bottom),
        (x, Drop::Left),
    ];
    let mut nearest = edges[0];
    for edge in edges.iter().skip(1) {
        if edge.0 < nearest.0 {
            nearest = *edge;
        }
    }
    Some(if nearest.0 < band {
        nearest.1
    } else {
        Drop::Tab(center)
    })
}

pub(super) fn body(
    app: &Crabdash,
    pane: u32,
    panel: Div,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let center = app
        .workspaces
        .layout()
        .pane(pane)
        .and_then(|(tabs, active)| tabs.iter().position(|tab| *tab == active))
        .map_or(0, |index| index);
    let drop = app
        .workspaces
        .drag_target
        .filter(|(id, _)| *id == pane)
        .map(|(_, drop)| drop);
    let hover_owner = cx.entity().downgrade();
    let body_bounds = Rc::new(Cell::new(None));
    let drop_bounds = body_bounds.clone();
    let measure = canvas(
        move |bounds, _, _| body_bounds.set(Some(bounds)),
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0();
    let overlay = div()
        .id(SharedString::from(format!("pane-{pane}-dock-overlay")))
        .absolute()
        .inset_0()
        .opacity(0.0)
        // Block content controls only while this pane is the current tab drop
        // target. Normal clicks and split-divider drags remain untouched.
        .drag_over::<DraggedTab>(move |this, drag, _, cx| {
            if valid_hover(&hover_owner, drag, cx) {
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
        .on_drop(cx.listener(move |app, drag: &DraggedTab, window, cx| {
            if !drag.validate(app, cx) {
                cx.stop_propagation();
                return;
            }
            // A quick drag can start on its final move, after GPUI's capture
            // phase has run. Resolve from the release location rather than a
            // missing or stale hover target.
            let Some(drop) = drop_bounds
                .get()
                .and_then(|bounds| target(bounds, window.mouse_position(), center))
            else {
                return;
            };
            app.workspaces.drag_target = None;
            app.drop_workspace_tab(drag.tab, pane, drop, cx);
            cx.notify();
            cx.stop_propagation();
        }));
    div()
        .id(SharedString::from(format!("pane-{pane}-body")))
        .flex_1()
        .min_h_0()
        .min_w_0()
        .relative()
        .overflow_hidden()
        .on_drag_move(
            cx.listener(move |app, event: &DragMoveEvent<DraggedTab>, _, cx| {
                let drag = event.drag(cx).clone();
                let candidate = if drag.validate(app, cx) {
                    target(event.bounds, event.event.position, center).map(|drop| (pane, drop))
                } else {
                    None
                };
                if let Some(candidate) = candidate {
                    if app.workspaces.drag_target != Some(candidate) {
                        app.workspaces.drag_target = Some(candidate);
                        cx.notify();
                    }
                } else if app.workspaces.drag_target.is_some_and(|(id, _)| id == pane) {
                    app.workspaces.drag_target = None;
                    cx.notify();
                }
            }),
        )
        .child(
            div()
                .size_full()
                .min_h_0()
                .min_w_0()
                .flex()
                .flex_col()
                .px(px(12.0))
                .pt(px(12.0))
                .child(panel),
        )
        .child(overlay)
        .child(measure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::test;

    #[test]
    fn geometric_targets_use_nearest_edge_and_leave_a_join_center() {
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(400.0), px(300.0)));
        for (position, expected) in [
            (point(px(210.0), px(30.0)), Drop::Top),
            (point(px(400.0), px(170.0)), Drop::Right),
            (point(px(210.0), px(310.0)), Drop::Bottom),
            (point(px(20.0), px(170.0)), Drop::Left),
            (point(px(210.0), px(170.0)), Drop::Tab(1)),
            // At a corner, Zed's tie order starts with the upper edge.
            (point(px(20.0), px(30.0)), Drop::Top),
        ] {
            assert_eq!(target(bounds, position, 1), Some(expected));
        }
        assert_eq!(target(bounds, point(px(9.0), px(170.0)), 1), None);
    }
}
