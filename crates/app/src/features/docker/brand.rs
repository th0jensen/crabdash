//! Official Docker mark, bundled for offline use on every desktop.
use gpui::{prelude::*, *};

pub(crate) const PATH: &str = "brands/docker.svg";
pub(crate) const BYTES: &[u8] = include_bytes!("../../../assets/brands/docker.svg");

pub(crate) fn icon(size: f32) -> Div {
    div()
        .flex_none()
        .w(rems(size / 16.0))
        .h(rems(size / 16.0))
        .flex()
        .items_center()
        .justify_center()
        .child(svg().path(PATH).size(rems(size / 16.0)).text_color(white()))
}
