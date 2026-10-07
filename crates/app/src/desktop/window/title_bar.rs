use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::Crabdash;
use crate::components::common::{control_tooltip, lucide_icon};
use crate::components::style;

pub fn platform_title_bar_height(window: &Window) -> Pixels {
    px(f32::from(window.rem_size()) * style::TITLE_BAR / 16.0)
}

pub(crate) fn render(
    app: &Crabdash,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let menus_visible = crate::desktop::menus::visible(app, window);
    div()
        .id("app-titlebar")
        .flex_none()
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |event, window, _| {
            if event.click_count == 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
        })
        .on_mouse_down(MouseButton::Right, |event, window, _| {
            window.show_window_menu(event.position);
        })
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
                .pl(px(super::platform::LEADING_PADDING))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .id("toggle-sidebar")
                        .tooltip(|_, cx| {
                            control_tooltip(
                                crate::desktop::menus::shortcut(
                                    "Toggle sidebar · ⌘S",
                                    "Toggle sidebar · Ctrl+B",
                                ),
                                cx,
                            )
                        })
                        .size(gpui::rems(style::CONTROL / 16.0))
                        .rounded(px(style::RADIUS))
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
                .when(!menus_visible, |this| {
                    this.child(
                        div()
                            .text_size(gpui::rems(style::TEXT / 16.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xC8C8C8))
                            .child("Crabdash"),
                    )
                })
                .when(menus_visible, |this| {
                    this.child(crate::desktop::menus::render(app, cx))
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .id("refresh-button")
                        .tooltip(|_, cx| {
                            control_tooltip(
                                crate::desktop::menus::shortcut("Refresh · ⌘R", "Refresh · Ctrl+R"),
                                cx,
                            )
                        })
                        .size(gpui::rems(style::CONTROL / 16.0))
                        .rounded(px(style::RADIUS))
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
                        .tooltip(|_, cx| {
                            control_tooltip(
                                crate::desktop::menus::shortcut(
                                    "Toggle terminal · ⌘J",
                                    "Toggle terminal · Ctrl+J",
                                ),
                                cx,
                            )
                        })
                        .size(gpui::rems(style::CONTROL / 16.0))
                        .flex_none()
                        .rounded(px(style::RADIUS))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(gpui::rems(style::TEXT / 16.0))
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
                        .child(lucide_icon(Icon::Terminal, style::ICON))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_quake_terminal(window, cx);
                        })),
                )
                .child(super::platform::controls(window)),
        )
}
