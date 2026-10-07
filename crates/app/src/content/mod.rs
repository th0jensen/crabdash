//! A single tab strip with a shared feature surface.
mod header;

use crate::app::{Crabdash, MainTab};
use crate::features::{disks, docker, services};
use gpui::{prelude::*, *};

pub fn render(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    let frame = crate::desktop::appearance::frame_inset(window) * 2.0;
    let width = (window.viewport_size().width
        - frame
        - if app.sidebar_collapsed {
            px(0.0)
        } else {
            app.sidebar_width
        })
    .max(px(0.0));
    let panel_width = (width - px(24.0)).max(px(0.0));
    let panel = match app.active_tab {
        MainTab::Docker => docker::render(app, window, cx, panel_width),
        MainTab::Disks => disks::render(app, window, cx, panel_width),
        MainTab::Services => services::render(app, window, cx, panel_width),
    };
    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .bg(rgb(0x181818))
        .child(header::render(app, window, width, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .relative()
                .px(px(12.0))
                .pt(px(12.0))
                .child(panel),
        )
}
