//! Docking composition. Domain controllers remain shared across unique feature panes.
mod header;

use crate::app::{Crabdash, MainTab};
use crate::components::style;
use crate::features::{
    disks, docker, services,
    workspaces::model::{Axis, Drop, Node},
};
use gpui::{prelude::*, *};
pub(crate) use header::DraggedTab;

#[derive(Clone)]
struct SplitResize {
    id: u32,
    owner: EntityId,
}

fn active_panel(
    app: &Crabdash,
    tab: MainTab,
    width: Pixels,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> Div {
    match tab {
        MainTab::Docker => docker::render(app, window, cx, width),
        MainTab::Disks => disks::render(app, window, cx, width),
        MainTab::Services => services::render(app, window, cx, width),
    }
}

fn drop_zone(
    pane: u32,
    drop: Drop,
    label: &'static str,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("pane-{pane}-drop-{label}")))
        .absolute()
        .rounded(px(style::CARD_RADIUS))
        .border_1()
        .border_color(rgb(style::BORDER))
        .bg(rgba(0x242424EE))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(style::TEXT_MUTED))
        .text_size(rems(style::META / 16.0))
        .drag_over::<DraggedTab>(|this, _, _, _| {
            this.bg(rgb(style::CONTROL_SELECTED_BG))
                .border_color(rgb(style::FOCUS_BORDER))
                .text_color(rgb(style::TEXT_SELECTED))
        })
        .child(label)
        .on_drop(cx.listener(move |app, drag: &DraggedTab, _, cx| {
            if drag.owner == cx.entity_id() {
                app.drop_workspace_tab(drag.tab, pane, drop, cx);
            }
            cx.stop_propagation();
        }))
}

fn docking_overlay(pane: u32, cx: &mut Context<Crabdash>) -> Div {
    div()
        .absolute()
        .inset_0()
        .child(
            drop_zone(pane, Drop::Left, "Split left", cx)
                .left(px(8.0))
                .top(relative(0.25))
                .w(relative(0.23))
                .h(relative(0.5)),
        )
        .child(
            drop_zone(pane, Drop::Right, "Split right", cx)
                .right(px(8.0))
                .top(relative(0.25))
                .w(relative(0.23))
                .h(relative(0.5)),
        )
        .child(
            drop_zone(pane, Drop::Top, "Split above", cx)
                .top(px(8.0))
                .left(relative(0.25))
                .w(relative(0.5))
                .h(relative(0.23)),
        )
        .child(
            drop_zone(pane, Drop::Bottom, "Split below", cx)
                .bottom(px(8.0))
                .left(relative(0.25))
                .w(relative(0.5))
                .h(relative(0.23)),
        )
        .child(
            drop_zone(pane, Drop::Tab(usize::MAX), "Join tabs", cx)
                .left(relative(0.25))
                .top(relative(0.25))
                .w(relative(0.5))
                .h(relative(0.5)),
        )
}

pub(crate) fn render_node(
    app: &Crabdash,
    node: &Node,
    width: Pixels,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    match node {
        Node::Pane { id, tabs, active } => {
            let pane = *id;
            let active_tab: MainTab = (*active).into();
            div()
                .id(SharedString::from(format!("workspace-pane-{pane}")))
                .size_full()
                .min_w_0()
                .min_h_0()
                .relative()
                .flex()
                .flex_col()
                .bg(rgb(0x181818))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, _, _, cx| {
                        app.select_pane_tab(pane, active_tab, cx);
                    }),
                )
                .on_drag_move(cx.listener(|_, _: &DragMoveEvent<DraggedTab>, _, cx| cx.notify()))
                .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, cx| cx.notify()))
                .child(header::render(app, pane, tabs, *active, window, width, cx))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .relative()
                        .px(px(12.0))
                        .pt(px(12.0))
                        .child(active_panel(
                            app,
                            active_tab,
                            (width - px(24.0)).max(px(0.0)),
                            window,
                            cx,
                        ))
                        .when(
                            cx.has_active_drag() && app.workspaces.dragging_tab,
                            |this| this.child(docking_overlay(pane, cx)),
                        ),
                )
        }
        Node::Split {
            id,
            axis,
            ratio,
            first,
            second,
        } => {
            let split = *id;
            let horizontal = *axis == Axis::Horizontal;
            let first_width = if horizontal { width * *ratio } else { width };
            let second_width = if horizontal {
                (width - first_width - px(5.0)).max(px(0.0))
            } else {
                width
            };
            let first_panel = render_node(app, first, first_width, window, cx);
            let second_panel = render_node(app, second, second_width, window, cx);
            let owner = cx.entity();
            div()
                .id(SharedString::from(format!("workspace-split-{split}")))
                .size_full()
                .min_w_0()
                .min_h_0()
                .flex()
                .when(!horizontal, |this| this.flex_col())
                .on_drag_move(
                    cx.listener(move |app, event: &DragMoveEvent<SplitResize>, _, cx| {
                        if event.drag(cx).id != split || event.drag(cx).owner != cx.entity_id() {
                            return;
                        }
                        let fraction = if horizontal {
                            f32::from(event.event.position.x - event.bounds.origin.x)
                                / f32::from(event.bounds.size.width)
                        } else {
                            f32::from(event.event.position.y - event.bounds.origin.y)
                                / f32::from(event.bounds.size.height)
                        };
                        if fraction.is_finite() {
                            app.workspaces.layout_mut().set_ratio(split, fraction);
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|app, _, _, cx| app.persist_workspace(cx)),
                )
                .child(
                    div()
                        .flex_none()
                        .min_w_0()
                        .min_h_0()
                        .when(horizontal, |this| this.w(relative(*ratio)).h_full())
                        .when(!horizontal, |this| this.h(relative(*ratio)).w_full())
                        .child(first_panel),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("workspace-divider-{split}")))
                        .flex_none()
                        .bg(rgb(style::BORDER))
                        .when(horizontal, |this| {
                            this.w(px(5.0)).h_full().cursor_col_resize()
                        })
                        .when(!horizontal, |this| {
                            this.h(px(5.0)).w_full().cursor_row_resize()
                        })
                        .hover(|this| this.bg(rgb(style::FOCUS_BORDER)))
                        .on_drag(
                            SplitResize {
                                id: split,
                                owner: owner.entity_id(),
                            },
                            move |_, _, _, cx| {
                                owner.update(cx, |app, cx| {
                                    app.workspaces.dragging_tab = false;
                                    cx.notify();
                                });
                                cx.new(|_| Empty)
                            },
                        ),
                )
                .child(div().flex_1().min_w_0().min_h_0().child(second_panel))
        }
    }
}

pub(crate) fn render_detached(
    app: &Crabdash,
    pane_id: u32,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> Div {
    let panel = app
        .workspaces
        .layout()
        .detached
        .iter()
        .find(|pane| pane.id == pane_id)
        .map(|pane| render_node(app, &pane.node, window.viewport_size().width, window, cx));
    div()
        .size_full()
        .bg(rgb(0x181818))
        .when_some(panel, |this, panel| this.child(panel))
}

pub fn render(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    let width = window.viewport_size().width
        - if app.sidebar_collapsed {
            px(0.0)
        } else {
            app.sidebar_width
        };
    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .bg(rgb(style::BORDER))
        .child(render_node(
            app,
            &app.workspaces.layout().root,
            width,
            window,
            cx,
        ))
}
