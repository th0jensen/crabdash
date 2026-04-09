mod disks;
mod docker;
mod docker_run_modal;
mod header;
mod services;
mod shared;
mod title_bar;

use gpui::prelude::*;
use gpui::*;

use crate::app::{Crabdash, MainTab};

fn active_panel(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    match app.active_tab {
        MainTab::Docker => docker::render(app, cx),
        MainTab::Disks => disks::render(app, cx),
        MainTab::Services => services::render(app, cx),
    }
}

pub fn render_docker_run_modal(
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
) -> impl IntoElement {
    docker_run_modal::render(app, cx)
}

pub fn render_title_bar(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    title_bar::render(app, window, cx)
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .bg(rgb(0x1C1C1E))
        .child(header::render(app, cx))
        .child(
            div()
                .flex_1()
                .w_full()
                .bg(rgb(0x1C1C1E))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_1()
                        .w_full()
                        .flex()
                        .flex_col()
                        .px(px(20.0))
                        .pt(px(20.0))
                        .child(div().flex_1().w_full().child(active_panel(app, cx))),
                ),
        )
}
