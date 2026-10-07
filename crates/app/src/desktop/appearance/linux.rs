use crate::components::style;
use gpui::{prelude::*, *};
pub(crate) fn root_background() -> Hsla {
    rgb(0x161616).into()
}
pub(crate) fn titlebar_background() -> Hsla {
    rgb(0x181818).into()
}
pub(crate) fn frame(root: Div, window: &Window) -> Div {
    root.when(!window.is_fullscreen(), |this| {
        this.border_1().border_color(rgb(style::BORDER))
    })
}
