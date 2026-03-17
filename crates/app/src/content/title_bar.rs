use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::Crabdash;
use crate::components::common::{lucide_icon, machine_icon};

#[cfg(not(target_os = "windows"))]
pub fn platform_title_bar_height(window: &Window) -> Pixels {
    (1.75 * window.rem_size()).max(px(34.))
}

pub(super) fn render(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    let selected_machine = app.selected_machine();

    div()
        .h(platform_title_bar_height(window))
        .px(px(14.0))
        .border_b_1()
        .border_color(rgb(0x242426))
        .bg(rgb(0x18181A))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .pl(px(72.0))
                .child(
                    div()
                        .id("toggle-sidebar")
                        .w(px(20.0))
                        .h(px(20.0))
                        .rounded(px(6.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0xE5E5EA))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x2C2C2E)).text_color(rgb(0xE5E5EA)))
                        .child(lucide_icon(
                            if app.sidebar_collapsed {
                                Icon::PanelLeftOpen
                            } else {
                                Icon::PanelLeftClose
                            },
                            12.0,
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_sidebar(cx);
                        })),
                )
                .text_sm()
                .text_color(rgb(0xE5E5EA))
                .child("Crabdash"),
        )
        .child(
            div()
                .px(px(10.0))
                .py(px(5.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_sm()
                .text_color(rgb(0xE5E5EA))
                .child(lucide_icon(machine_icon(selected_machine.kind), 12.0))
                .child(format!("{}", selected_machine.system_info.machine_name,)),
        )
}
