use gpui::{prelude::*, *};
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
pub(super) fn configure(options: &mut WindowOptions) {
    options.window_background = WindowBackgroundAppearance::Transparent;
}
pub(super) fn prepare(window: &mut Window, cx: &mut App) {
    crate::desktop::appearance::prepare(window);
    window.on_window_should_close(cx, crate::desktop::tray::should_close);
}
pub(super) fn register_lifecycle(_: &mut App) {}
pub(super) fn activate_window(window: &mut Window, _: Option<&str>) {
    if let Some(native) = native_window(window) {
        if native.isMiniaturized() {
            native.deminiaturize(None);
        }
    }
    window.activate_window();
}
pub(super) fn zoom(window: &mut Window) {
    window.zoom_window();
}
fn native_window(window: &Window) -> Option<Retained<NSWindow>> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: GPUI supplies its live NSView and callers run on the UI thread.
    let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
    view.window()
}
pub(super) fn hide_to_tray(window: &mut Window) {
    if let Some(native) = native_window(window) {
        // Hiding the application would also hide other dashboard
        // windows. Keep backgrounding local to the window that was closed.
        native.orderOut(None);
    } else {
        window.minimize_window();
    }
}
pub(super) fn controls(_: &Window) -> Div {
    div()
}
pub(crate) fn resize_handles(_: &Window) -> Div {
    div().absolute().inset_0().size_full()
}
pub(super) const LEADING_PADDING: f32 = 70.0;
