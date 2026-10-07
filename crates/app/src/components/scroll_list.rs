use gpui::prelude::*;
use gpui::*;

use crate::app::Crabdash;

/// Fixed chrome around a body that owns its own bounded scrolling, such as a
/// variable-height GPUI list. Do not add another scrolling ancestor here.
pub(crate) fn bounded(header: AnyElement, body: impl IntoElement, is_scrolled: bool) -> Div {
    div()
        .relative()
        .size_full()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .flex_none()
                .pb(px(12.0))
                .border_b_1()
                .border_color(if is_scrolled {
                    rgb(crate::components::style::BORDER)
                } else {
                    rgba(0x00000000)
                })
                .child(header),
        )
        .child(div().flex_1().min_h_0().min_w_0().w_full().child(body))
}

pub fn render(
    id: impl Into<ElementId>,
    scroll_handle: &ScrollHandle,
    header: Option<AnyElement>,
    body: impl IntoElement,
    cx: &mut Context<Crabdash>,
) -> Div {
    let max_scroll = scroll_handle.max_offset().height;
    let is_scrollable = max_scroll > px(2.0);
    let is_scrolled = is_scrollable && scroll_handle.offset().y < px(0.0);
    let scroll_handle_for_wheel = scroll_handle.clone();

    div()
        .relative()
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .when_some(header, |this, header| {
            this.child(
                div()
                    .w_full()
                    .flex_none()
                    .pb(px(12.0))
                    .border_b_1()
                    .border_color(if is_scrolled {
                        rgb(crate::components::style::BORDER)
                    } else {
                        rgba(0x00000000)
                    })
                    .child(header),
            )
        })
        .child(
            div()
                .id(id)
                .flex_1()
                .min_h_0()
                .w_full()
                .track_scroll(scroll_handle)
                .overflow_y_scroll()
                .on_scroll_wheel(cx.listener(move |_, _: &ScrollWheelEvent, _, cx| {
                    // GPUI runs this element's native scroll handler first in
                    // the bubble phase. Clamp its result without adding twice.
                    let current_offset = scroll_handle_for_wheel.offset();
                    let max_offset = scroll_handle_for_wheel.max_offset();
                    let next_y = current_offset.y.max(-max_offset.height).min(px(0.0));
                    if next_y != current_offset.y {
                        scroll_handle_for_wheel.set_offset(point(current_offset.x, next_y));
                    }
                    // The header's separator is outside the scrolling element.
                    cx.notify();
                    cx.stop_propagation();
                }))
                .child(div().w_full().pb(px(32.0)).child(body)),
        )
}
