use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::{Crabdash, MainTab};
use crate::components::common::lucide_icon;

fn tab_button(tab: MainTab, active: bool, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let bg = if active { rgb(0x2C2C2E) } else { rgb(0x1C1C1E) };
    let color = if active { rgb(0xFFFFFF) } else { rgb(0xAEAEB2) };

    div()
        .id(SharedString::from(format!(
            "tab-{}",
            tab.label().to_lowercase()
        )))
        .flex_none()
        .h(px(34.0))
        .px(px(14.0))
        .bg(bg)
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .rounded(px(8.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2A2C)))
        .child(
            div()
                .h_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_sm()
                .text_color(color)
                .child(lucide_icon(tab.icon(), 14.0))
                .child(tab.label().to_string()),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.active_tab = tab;
            this.refresh_services();
            cx.notify();
        }))
}

fn refresh_button(cx: &mut Context<Crabdash>) -> impl IntoElement {
    let color = rgb(0xAEAEB2);

    div()
        .id("refresh-button")
        .flex_none()
        .h(px(34.0))
        .px(px(14.0))
        .bg(rgb(0x1C1C1E))
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .rounded(px(8.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2A2C)))
        .child(
            div()
                .h_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_sm()
                .text_color(color)
                .child(lucide_icon(Icon::RefreshCw, 14.0))
                .child("Refresh"),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.refresh_services();
            cx.notify();
        }))
}

pub(super) fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .px(px(20.0))
        .py(px(12.0))
        .border_b_1()
        .border_color(rgb(0x3A3A3C))
        .flex()
        .justify_between()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .gap(px(10.0))
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
        .child(refresh_button(cx))
}
