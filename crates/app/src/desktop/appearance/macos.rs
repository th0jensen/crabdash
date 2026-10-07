//! AppKit owns sidebar and toolbar materials; dashboard content stays opaque.
use gpui::{Div, Hsla, Pixels, Window, px, rgb};

pub(crate) fn root_background() -> Hsla {
    rgb(crate::components::style::CONTENT).into()
}
pub(crate) fn titlebar_background() -> Hsla {
    rgb(crate::components::style::CHROME).into()
}
pub(crate) fn frame(root: Div, _: &Window) -> Div {
    root
}
pub(crate) fn frame_inset(_: &Window) -> Pixels {
    px(0.0)
}
