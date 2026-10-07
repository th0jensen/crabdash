use gpui::prelude::*;
use gpui::*;

use crate::app::{Crabdash, MainTab};
use crate::components::common::lucide_icon;
use crate::components::style;

fn tab_button(
    tab: MainTab,
    active: bool,
    hints: bool,
    cx: &mut Context<Crabdash>,
) -> impl IntoElement {
    let width = crate::features::preferences::current(cx).tab_width;
    div()
        .id(SharedString::from(format!(
            "tab-{}",
            tab.label().to_lowercase()
        )))
        .h_full()
        .w(gpui::rems(width / 16.0))
        .flex_none()
        .px(px(12.0))
        .relative()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(gpui::rems(style::TEXT / 16.0))
        .text_color(if active {
            rgb(style::TEXT_SELECTED)
        } else {
            rgb(style::TEXT_MUTED)
        })
        .when(active, |this| {
            this.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left(px(12.0))
                    .right(px(12.0))
                    .h(px(2.0))
                    .bg(rgb(style::TAB_INDICATOR)),
            )
        })
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(rgb(style::SURFACE_HOVER))
                .text_color(rgb(style::TEXT_SELECTED))
        })
        .child(lucide_icon(tab.icon(), style::ICON))
        .child(tab.label().to_string())
        .child(div().flex_1())
        .when(hints, |this| {
            this.child(
                div()
                    .text_size(gpui::rems(style::META / 16.0))
                    .text_color(rgb(0x888888))
                    .child(tab.shortcut()),
            )
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.active_tab = tab;
            this.refresh_services(cx);
            cx.notify();
        }))
}

pub(super) fn render(app: &Crabdash, window: &Window, cx: &mut Context<Crabdash>) -> Div {
    let hints =
        app.preferences.always_show_shortcuts || window.modifiers().alt || app.open_menu.is_some();
    div()
        .h(gpui::rems(style::BAR / 16.0))
        .flex_none()
        .bg(rgb(0x181818))
        .border_b_1()
        .border_color(rgb(0x2B2B2B))
        .flex()
        .items_center()
        .child(tab_button(
            MainTab::Docker,
            app.active_tab == MainTab::Docker,
            hints,
            cx,
        ))
        .child(tab_button(
            MainTab::Disks,
            app.active_tab == MainTab::Disks,
            hints,
            cx,
        ))
        .child(tab_button(
            MainTab::Services,
            app.active_tab == MainTab::Services,
            hints,
            cx,
        ))
        .child(div().flex_1().h_full())
}
