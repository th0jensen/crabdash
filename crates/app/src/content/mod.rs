//! Main panel composition and navigation; domain views live in features.
mod header;

use crate::features::{disks, docker, services};
use gpui::prelude::*;
use gpui::*;

use crate::app::{Crabdash, MainTab};

fn active_panel(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    match app.active_tab {
        MainTab::Docker => docker::render(app, window, cx),
        MainTab::Disks => disks::render(app, window, cx),
        MainTab::Services => services::render(app, window, cx),
    }
}

pub fn render(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .bg(rgb(0x181818))
        .child(header::render(app, window, cx))
        .child(
            div()
                .flex_1()
                .w_full()
                .min_h_0()
                .px(px(16.0))
                .pt(px(16.0))
                .child(active_panel(app, window, cx)),
        )
}
