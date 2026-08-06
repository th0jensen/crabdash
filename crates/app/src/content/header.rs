use gpui::prelude::*;
use gpui::*;

use crate::app::{Crabdash, MainTab};
use crate::components::common::lucide_icon;

fn tab_button(tab: MainTab, active: bool, cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .id(SharedString::from(format!(
            "tab-{}",
            tab.label().to_lowercase()
        )))
        .h_full()
        .px(px(18.0))
        .border_r_1()
        .border_color(rgb(0x2B2B2B))
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(px(14.0))
        .text_color(if active { rgb(0xE0E0E0) } else { rgb(0x8A8A8A) })
        .when(active, |this| this.bg(rgb(0x242424)))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x222222)).text_color(rgb(0xD4D4D4)))
        .child(lucide_icon(tab.icon(), 14.0))
        .child(tab.label().to_string())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.active_tab = tab;
            this.refresh_services(cx);
            cx.notify();
        }))
}

pub(super) fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .h(px(44.0))
        .flex_none()
        .bg(rgb(0x181818))
        .border_b_1()
        .border_color(rgb(0x2B2B2B))
        .flex()
        .items_center()
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
        ))
        .child(div().flex_1().h_full())
}
