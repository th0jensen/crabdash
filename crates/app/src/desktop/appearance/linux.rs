use crate::components::style;
use gpui::{prelude::*, *};
pub(crate) fn root_background() -> Hsla {
    crate::components::style::rgb(style::CONTENT).into()
}
pub(crate) fn titlebar_background() -> Hsla {
    crate::components::style::rgb(style::CHROME).into()
}
pub(crate) fn frame(root: Div, window: &Window) -> Div {
    root.when(!window.is_fullscreen(), |this| {
        this.border_1()
            .border_color(crate::components::style::rgb(style::BORDER))
    })
}

/// Insets introduced by the application-owned border around the client area.
pub(crate) fn frame_inset(window: &Window) -> Pixels {
    px(if window.is_fullscreen() { 0.0 } else { 1.0 })
}
