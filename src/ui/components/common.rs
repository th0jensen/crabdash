use gpui::prelude::*;
use gpui::*;

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
) -> Stateful<Div> {
    let label = label.into();
    let bg = if primary {
        rgb(0x0A84FF)
    } else {
        rgb(0x2C2C2E)
    };
    let hover = if primary {
        rgb(0x3B9CFF)
    } else {
        rgb(0x3A3A3C)
    };
    let border = if primary {
        rgb(0x0A84FF)
    } else {
        rgb(0x3A3A3C)
    };

    div()
        .id(id)
        .h(px(34.0))
        .px(px(14.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .rounded(px(8.0))
        .text_sm()
        .text_color(white())
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(label)
}
