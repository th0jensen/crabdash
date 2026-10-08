//! In-window pane composition; feature state stays scoped by unique domain tabs.
mod docking;
mod geometry;
use geometry::minimum_extent;
pub(crate) use geometry::minimum_visible_height;
mod header;
mod resize;

use crate::app::{Crabdash, MainTab};
use crate::components::style;
use crate::features::{
    disks, docker, services, system,
    workspaces::model::{Axis, Node},
};
use gpui::{prelude::*, *};
use resize::usable_ratio;
use std::{cell::Cell, rc::Rc};

#[cfg(test)]
pub(crate) fn minimum_pane_extent(node: &Node, axis: Axis, scale: f32) -> f32 {
    geometry::minimum_extent(node, axis, scale)
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
            let geometry = resize::Geometry {
                axis: *axis,
                first_min,
                second_min,
            };
            let bounds = Rc::new(Cell::new(None));
            let measure_bounds = bounds.clone();
            let measure = canvas(
                move |bounds, _, _| measure_bounds.set(Some(bounds)),
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0();
            let divider = resize::divider(app, id, first_extent, geometry, bounds, cx);
            resize::moves(
                div()
                    .id(SharedString::from(format!("workspace-split-{id}")))
                    .w(width)
                    .h(height)
                    .flex_none()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .when(!horizontal, |this| this.flex_col())
                    .relative()
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |app, _, _, cx| {
                            if app
                                .workspaces
                                .split_resize_grab
                                .is_some_and(|(anchor, _)| anchor == id)
                            {
                                app.workspaces.split_resize_grab = None;
                            }
                            if app.workspaces.resizing_split == Some(id) {
                                app.workspaces.resizing_split = None;
                                app.persist_workspace(cx);
                            }
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(move |app, _, _, cx| {
                            if app
                                .workspaces
                                .split_resize_grab
                                .is_some_and(|(anchor, _)| anchor == id)
                            {
                                app.workspaces.split_resize_grab = None;
                            }
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
                            .when(!horizontal, |this| this.h(px(1.0)).w_full()),
                    )
                    .child(div().flex_1().min_w_0().min_h_0().child(second_panel))
                    .child(divider)
                    .child(measure),
                id,
                geometry,
                cx,
            )
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
        }
        // Docking and scrolling use only the viewport above the visible drawer.
        - if app.quake_terminal_open && app.active_quake_terminal().is_some() {
            app.quake_height
        } else {
            px(0.0)
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
