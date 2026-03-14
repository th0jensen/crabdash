use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::Crabdash;
use crate::components::common::lucide_icon;

pub fn render(message: impl Into<SharedString>, cx: &mut Context<Crabdash>) -> Div {
    let message = message.into();

    div()
        .w(px(360.0))
        .p(px(14.0))
        .bg(rgb(0x18181A))
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .rounded(px(12.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(0xFF453A))
                                .child(lucide_icon(Icon::CircleAlert, 16.0)),
                        )
                        .child(div().text_xs().text_color(rgb(0xFF453A)).child("Error")),
                )
                .child(
                    div()
                        .id("dismiss-status-toast")
                        .flex_none()
                        .w(px(20.0))
                        .h(px(20.0))
                        .rounded(px(6.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_color(white())
                        .hover(|style| style.bg(rgb(0x2C2C2E)).text_color(white()))
                        .child(lucide_icon(Icon::X, 12.0))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_status_message();
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .w_full()
                .whitespace_normal()
                .text_sm()
                .text_color(white())
                .child(message),
        )
}
