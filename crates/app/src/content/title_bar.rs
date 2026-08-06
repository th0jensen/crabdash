use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::Crabdash;
use crate::components::common::lucide_icon;

#[cfg(not(target_os = "windows"))]
pub fn platform_title_bar_height(window: &Window) -> Pixels {
    (2.1 * window.rem_size()).max(px(42.0))
}

pub(super) fn render(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    div()
        .h(platform_title_bar_height(window))
        .px(px(10.0))
        .border_b_1()
        .border_color(rgb(0x2B2B2B))
        .bg(rgb(0x181818))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .pl(px(70.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .id("toggle-sidebar")
                        .size(px(28.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0xA0A0A0))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x2A2A2A)).text_color(rgb(0xD4D4D4)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .child(lucide_icon(
                            if app.sidebar_collapsed {
                                Icon::PanelLeftOpen
                            } else {
                                Icon::PanelLeftClose
                            },
                            14.0,
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_sidebar(cx);
                        })),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0xC8C8C8))
                        .child("Crabdash"),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .id("refresh-button")
                        .size(px(30.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0xA0A0A0))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x2A2A2A)).text_color(rgb(0xE8E8E8)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .child(lucide_icon(Icon::RefreshCw, 14.0))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.refresh_services(cx);
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("toggle-quake-terminal")
                        .h(px(30.0))
                        .px(px(10.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(14.0))
                        .text_color(if app.quake_terminal_open {
                            rgb(0xD4D4D4)
                        } else {
                            rgb(0xA0A0A0)
                        })
                        .when(app.quake_terminal_open, |this| this.bg(rgb(0x2A2A2A)))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x2A2A2A)).text_color(rgb(0xD4D4D4)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .child(lucide_icon(Icon::Terminal, 14.0))
                        .child("Terminal")
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(0x707070))
                                .child("⌘J"),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_quake_terminal(window, cx);
                        })),
                ),
        )
}
