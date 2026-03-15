use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::{Crabdash, MainTab};
use crate::components::common::lucide_icon;

fn tab_button(tab: MainTab, active: bool, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let bg = rgb(0x1C1C1E);
    let border = rgb(0x2F2F31);
    let color = if active { rgb(0xFFFFFF) } else { rgb(0x8E8E93) };

    div()
        .id(SharedString::from(format!(
            "tab-{}",
            tab.label().to_lowercase()
        )))
        .flex_none()
        .flex()
        .items_center()
        .justify_start()
        .h(px(32.0))
        .px(px(22.0))
        .bg(bg)
        .relative()
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x202022)))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(1.0))
                .bg(border),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .bottom_0()
                .w(px(1.0))
                .bg(border),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(1.0))
                .bg(border),
        )
        .child(
            div()
                .gap(px(6.0))
                .flex()
                .items_center()
                .text_sm()
                .text_color(color)
                .child(lucide_icon(tab.icon(), 13.0))
                .child(tab.label().to_string()),
        )
        .when(!active, |this| {
            this.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(1.0))
                    .bg(border),
            )
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.active_tab = tab;
            this.refresh_services();
            cx.notify();
        }))
}

#[allow(dead_code)]
fn refresh_button(cx: &mut Context<Crabdash>) -> impl IntoElement {
    let color = rgb(0xAEAEB2);

    div()
        .id("refresh-button")
        .flex_none()
        .h(px(32.0))
        .px(px(10.0))
        .bg(rgb(0x1C1C1E))
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x202022)))
        .child(
            div()
                .h_full()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_sm()
                .text_color(color)
                .child(lucide_icon(Icon::RefreshCw, 13.0))
                .child("Refresh"),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.refresh_services();
            cx.notify();
        }))
}

pub(super) fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .h(px(32.0))
        .flex()
        .items_end()
        .child(
            div()
                .flex()
                .gap(px(0.0))
                .child(tab_button(
                    MainTab::Docker,
                    app.active_tab == MainTab::Docker,
                    cx,
                ))
                .child(tab_button(
                    MainTab::Disks,
                    app.active_tab == MainTab::Disks,
                    cx,
                ))
                .child(tab_button(
                    MainTab::Services,
                    app.active_tab == MainTab::Services,
                    cx,
                )),
        )
        .child(
            div()
                .flex_1()
                .h_full()
                .pr(px(20.0))
                .border_b_1()
                .border_color(rgb(0x2F2F31)),
        )
    // .child(refresh_button(cx))
}
