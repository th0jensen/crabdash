//! A shared divider hit area, painted above both adjoining panes.
use crate::layout::Axis;
use gpui::{prelude::*, *};

pub(crate) fn render(id: impl Into<ElementId>, axis: Axis, first: Pixels) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .occlude()
        .when(axis == Axis::Horizontal, |this| {
            this.left(first - px(3.0))
                .w(px(7.0))
                .top_0()
                .bottom_0()
                .cursor_col_resize()
        })
        .when(axis == Axis::Vertical, |this| {
            this.top(first - px(3.0))
                .h(px(7.0))
                .left_0()
                .right_0()
                .cursor_row_resize()
        })
}
