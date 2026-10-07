//! In-window pane composition; feature state stays scoped by unique domain tabs.
mod docking;
mod header;

use crate::app::{Crabdash, MainTab};
use crate::components::style;
use crate::features::{
    disks, docker, services, system,
    workspaces::model::{Axis, Node},
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};
use uuid::Uuid;

#[derive(Clone)]
struct SplitResize {
    id: u32,
    owner: EntityId,
    workspace: Uuid,
    revision: Rc<Cell<u64>>,
}
impl Render for SplitResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn minimum_extent(node: &Node, axis: Axis, scale: f32) -> f32 {
    match node {
        Node::Pane { .. } => match axis {
            Axis::Horizontal => 200.0 * scale + 24.0,
            Axis::Vertical => 200.0 * scale + 12.0,
        },
        Node::Split {
            axis: split_axis,
            first,
            second,
            ..
        } => {
            let first = minimum_extent(first, axis, scale);
            let second = minimum_extent(second, axis, scale);
            if *split_axis == axis {
                first + second + 1.0
            } else {
                first.max(second)
            }
        }
    }
}

fn usable_ratio(ratio: f32, extent: f32, first_min: f32, second_min: f32) -> f32 {
    let available = (extent - 1.0).max(0.0);
    if available >= first_min + second_min {
        ratio.clamp(first_min / available, 1.0 - second_min / available)
    } else {
        first_min / (first_min + second_min)
    }
}

fn panel(
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
        MainTab::System => div()
            .w(width)
            .h_full()
            .min_w_0()
            .min_h_0()
            .child(system::render(app, window, cx, width)),
    }
}

fn render_node(
    app: &Crabdash,
    node: &Node,
    width: Pixels,
    height: Pixels,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    match node {
        Node::Pane { id, tabs, active } => {
            let pane = *id;
            let feature = panel(
                app,
                (*active).into(),
                (width - px(24.0)).max(px(0.0)),
                window,
                cx,
            );
            div()
                .id(SharedString::from(format!("workspace-pane-{pane}")))
                // Percentage heights can resolve against System's intrinsic
                // scroll content in a nested block wrapper. Honor the split's
                // allocated geometry so each domain scrolls inside its pane.
                .w(width)
                .h(height)
                .flex_none()
                .min_w_0()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .flex()
                .flex_col()
                .bg(rgb(style::CONTENT))
                .capture_any_mouse_down(cx.listener(move |app, _, _, cx| {
                    app.focus_workspace_pane(pane, cx);
                }))
                .child(header::render(app, pane, tabs, *active, window, width, cx))
                .child(docking::body(app, pane, feature, cx))
        }
        Node::Split {
            id,
            axis,
            ratio,
            first,
            second,
        } => {
            let id = *id;
            let horizontal = *axis == Axis::Horizontal;
            let scale = f32::from(window.rem_size()) / 16.0;
            let first_min = minimum_extent(first, *axis, scale);
            let second_min = minimum_extent(second, *axis, scale);
            let extent = if horizontal { width } else { height };
            let ratio = usable_ratio(*ratio, f32::from(extent), first_min, second_min);
            let first_extent = (extent - px(1.0)).max(px(0.0)) * ratio;
            let second_extent = (extent - px(1.0) - first_extent).max(px(0.0));
            let first_width = if horizontal { first_extent } else { width };
            let second_width = if horizontal { second_extent } else { width };
            let first_height = if horizontal { height } else { first_extent };
            let second_height = if horizontal { height } else { second_extent };
            let first_panel = render_node(app, first, first_width, first_height, window, cx);
            let second_panel = render_node(app, second, second_width, second_height, window, cx);
            let owner = cx.entity_id();
            let workspace = app.workspaces.store.active;
            let revision = app.workspaces.revision();
            let entity = cx.entity().downgrade();
            let divider = div()
                .id(SharedString::from(format!("workspace-divider-{id}")))
                .absolute()
                .when(horizontal, |this| {
                    this.left(-px(3.0))
                        .right(-px(3.0))
                        .top_0()
                        .bottom_0()
                        .cursor_col_resize()
                })
                .when(!horizontal, |this| {
                    this.top(-px(3.0))
                        .bottom(-px(3.0))
                        .left_0()
                        .right_0()
                        .cursor_row_resize()
                })
                .on_drag(
                    SplitResize {
                        id,
                        owner,
                        workspace,
                        revision: Rc::new(Cell::new(revision)),
                    },
                    move |drag, _, _, cx| {
                        entity
                            .update(cx, |app, cx| {
                                // Mouse-down can persist a new focused pane
                                // before the next frame replaces this payload.
                                drag.revision.set(app.workspaces.revision());
                                app.workspaces.drag_target = None;
                                app.workspaces.resizing_split = Some(id);
                                cx.notify();
                            })
                            .ok();
                        cx.new(|_| drag.clone())
                    },
                );
            div()
                .id(SharedString::from(format!("workspace-split-{id}")))
                .w(width)
                .h(height)
                .flex_none()
                .min_w_0()
                .min_h_0()
                .flex()
                .when(!horizontal, |this| this.flex_col())
                .on_drag_move(
                    cx.listener(move |app, event: &DragMoveEvent<SplitResize>, _, cx| {
                        let drag = event.drag(cx);
                        if drag.id != id
                            || drag.owner != cx.entity_id()
                            || drag.workspace != app.workspaces.store.active
                            || drag.revision.get() != app.workspaces.revision()
                            || app.workspaces.resizing_split != Some(id)
                        {
                            return;
                        }
                        app.workspaces.drag_target = None;
                        let extent = if horizontal {
                            event.bounds.size.width
                        } else {
                            event.bounds.size.height
                        };
                        let position = if horizontal {
                            event.event.position.x - event.bounds.origin.x
                        } else {
                            event.event.position.y - event.bounds.origin.y
                        };
                        let fraction = f32::from(position - px(0.5)) / f32::from(extent - px(1.0));
                        if fraction.is_finite() {
                            let ratio =
                                usable_ratio(fraction, f32::from(extent), first_min, second_min);
                            if app.workspaces.layout_mut().set_ratio(id, ratio) {
                                cx.notify();
                            }
                        }
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |app, _, _, cx| {
                        if app.workspaces.resizing_split == Some(id) {
                            app.workspaces.resizing_split = None;
                            app.persist_workspace(cx);
                        }
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(move |app, _, _, cx| {
                        if app.workspaces.resizing_split == Some(id) {
                            app.workspaces.resizing_split = None;
                            app.persist_workspace(cx);
                        }
                    }),
                )
                .child(
                    div()
                        .flex_none()
                        .min_w_0()
                        .min_h_0()
                        .when(horizontal, |this| this.w(first_extent).h_full())
                        .when(!horizontal, |this| this.h(first_extent).w_full())
                        .child(first_panel),
                )
                .child(
                    div()
                        .flex_none()
                        .relative()
                        .bg(rgb(style::BORDER))
                        .when(horizontal, |this| this.w(px(1.0)).h_full())
                        .when(!horizontal, |this| this.h(px(1.0)).w_full())
                        .child(divider),
                )
                .child(div().flex_1().min_w_0().min_h_0().child(second_panel))
        }
    }
}

pub fn render(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    let frame = crate::desktop::appearance::frame_inset(window) * 2.0;
    let width = (window.viewport_size().width
        - frame
        - if app.sidebar_collapsed || crate::desktop::shell::is_native(app) {
            px(0.0)
        } else {
            app.sidebar_width
        })
    .max(px(0.0));
    let height = (window.viewport_size().height
        - frame
        - if crate::desktop::shell::is_native(app) {
            px(0.0)
        } else {
            px(f32::from(window.rem_size()) * style::TITLE_BAR / 16.0)
        })
    .max(px(0.0));
    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .bg(rgb(style::CONTENT))
        .child(render_node(
            app,
            &app.workspaces.layout().root,
            width,
            height,
            window,
            cx,
        ))
}
