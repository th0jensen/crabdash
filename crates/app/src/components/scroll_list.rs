use gpui::prelude::*;
use gpui::*;

use crate::app::Crabdash;

pub fn render(
    id: impl Into<ElementId>,
    scroll_handle: &ScrollHandle,
    header: Option<AnyElement>,
    body: impl IntoElement,
    cx: &mut Context<Crabdash>,
) -> Div {
    const HEADER_HEIGHT: f32 = 46.0;

    let max_scroll = scroll_handle.max_offset().height;
    let is_scrollable = max_scroll > px(2.0);
    let is_scrolled = is_scrollable && scroll_handle.offset().y < px(0.0);
    let has_header = header.is_some();
    let scroll_handle_for_wheel = scroll_handle.clone();

    div()
        .relative()
        .size_full()
        .when_some(header, |this, header| {
            this.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(HEADER_HEIGHT))
                    .pb(px(12.0))
                    .child(header),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(HEADER_HEIGHT - 1.0))
                    .h(px(1.0))
                    .bg(if is_scrolled {
                        rgb(0x2F2F31)
                    } else {
                        rgba(0x00000000)
                    }),
            )
        })
        .child(
            div()
                .id(id)
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .top(if has_header {
                    px(HEADER_HEIGHT)
                } else {
                    px(0.0)
                })
                .w_full()
                .track_scroll(scroll_handle)
                .when(is_scrollable, |this| {
                    this.overflow_y_scroll().on_scroll_wheel(cx.listener(
                        move |_, event: &ScrollWheelEvent, window, cx| {
                            let delta = event.delta.pixel_delta(window.line_height());
                            let current_offset = scroll_handle_for_wheel.offset();
                            let max_offset = scroll_handle_for_wheel.max_offset();
                            let next_y = (current_offset.y + delta.y)
                                .max(-max_offset.height)
                                .min(px(0.0));

                            if next_y != current_offset.y {
                                scroll_handle_for_wheel.set_offset(point(current_offset.x, next_y));
                                cx.notify();
                            }

                            cx.stop_propagation();
                        },
                    ))
                })
                .when(!is_scrollable, |this| this.overflow_hidden())
                .child(div().w_full().pb(px(50.0)).child(body)),
        )
}
