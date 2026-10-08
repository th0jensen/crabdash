//! Shared compact previews for dashboard and terminal tab drags.
use super::{common::clipped_text, style};
use gpui::{prelude::*, *};

pub(crate) fn render(title: impl Into<SharedString>, icon: Div) -> Div {
    div()
        .h(rems(style::BAR / 16.0))
        .max_w(rems(280.0 / 16.0))
        .min_w_0()
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(rems(4.0 / 16.0))
        .bg(rgb(style::SURFACE_HOVER))
        .text_size(rems(style::TEXT / 16.0))
        .text_color(rgb(style::TEXT_SELECTED))
        .shadow_md()
        .child(
            div()
                .size(rems(style::ICON / 16.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(icon),
        )
        .child(clipped_text(title).flex_shrink())
}
