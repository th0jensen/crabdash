use gpui::prelude::*;
use gpui::*;

pub(super) fn placeholder_card(title: &str, description: &str) -> Div {
    div()
        .w_full()
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x2F2F31))
        .rounded(px(8.0))
        .px(px(14.0))
        .py(px(12.0))
        .flex()
        .justify_between()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(div().text_sm().text_color(white()).child(title.to_string()))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x8E8E93))
                        .child(description.to_string()),
                ),
        )
}

pub(super) fn error_panel(title: &str, error: String) -> Div {
    div()
        .bg(rgb(0x47232B))
        .border_1()
        .border_color(rgb(0x4A252C))
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xFF9F99))
                .child(title.to_string()),
        )
        .child(div().text_sm().text_color(rgb(0xF2B6B2)).child(error))
}
