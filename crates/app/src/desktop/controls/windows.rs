use super::Control;
use crate::app::Crabdash;
use gpui::{AnyElement, Context, IntoElement, Window};
pub(super) fn render(
    control: Control,
    app: &Crabdash,
    _: &mut Window,
    cx: &mut Context<Crabdash>,
) -> AnyElement {
    super::fallback(control, app, cx).into_any_element()
}
