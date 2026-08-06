mod disks;
mod docker;
mod docker_run_modal;
mod header;
mod services;
mod shared;
pub mod terminal;
mod title_bar;

use gpui::prelude::*;
use gpui::*;

use crate::app::{Crabdash, MainTab};

fn active_panel(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    match app.active_tab {
        MainTab::Docker => docker::render(app, window, cx),
        MainTab::Disks => disks::render(app, cx),
        MainTab::Services => services::render(app, cx),
    }
}

pub fn render_docker_run_modal(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
    docker_run_modal::render(app, cx)
}

pub fn render_title_bar(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    title_bar::render(app, window, cx)
}

// pub fn render_logs_modal(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
//     docker::render_logs_modal(app, cx)
// }

pub fn render(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .bg(rgb(0x181818))
        .child(header::render(app, cx))
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
