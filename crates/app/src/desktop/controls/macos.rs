//! GPUI fallback used only if the native AppKit shell cannot be installed.
use crate::app::Crabdash;
use gpui::{prelude::*, *};

pub(super) fn render(
    control: super::Control,
    app: &Crabdash,
    _: &mut Window,
    cx: &mut Context<Crabdash>,
) -> AnyElement {
    super::fallback(control, app, cx).into_any_element()
}
