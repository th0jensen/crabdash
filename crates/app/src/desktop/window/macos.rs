use gpui::{prelude::*, *};
pub(super) fn configure(options: &mut WindowOptions) {
    options.window_background = WindowBackgroundAppearance::Transparent;
}
pub(super) fn prepare(window: &mut Window, cx: &mut App) {
    crate::desktop::appearance::prepare(window);
    window.on_window_should_close(cx, crate::desktop::tray::should_close);
}
pub(super) fn register_lifecycle(_: &mut App) {}
pub(super) fn activate_window(window: &mut Window, _: Option<&str>) {
    window.activate_window();
}
pub(super) fn controls(_: &Window) -> Div {
    div()
}
pub(crate) fn resize_handles(_: &Window) -> Div {
    div().absolute().inset_0().size_full()
}
pub(super) const LEADING_PADDING: f32 = 70.0;
