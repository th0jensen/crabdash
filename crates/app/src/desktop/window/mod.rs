#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;
#[cfg(target_os = "linux")]
mod activation;
mod title_bar;
use gpui::*;
pub(crate) use platform::resize_handles;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) use title_bar::platform_title_bar_height;
pub(crate) use title_bar::render;
pub(crate) fn options(cx: &mut App, minimised: bool) -> WindowOptions {
    let mut options = WindowOptions {
        app_id: Some("crabdash".into()),
        focus: !minimised,
        window_bounds: Some(WindowBounds::centered(size(px(1100.0), px(740.0)), cx)),
        window_min_size: Some(size(px(680.0), px(540.0))),
        titlebar: Some(TitlebarOptions {
            title: Some("Crabdash".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.0), px(11.0))),
            ..Default::default()
        }),
        ..WindowOptions::default()
    };
    platform::configure(&mut options);
    options
}
pub(crate) fn prepare(window: &mut Window, cx: &mut App) {
    platform::prepare(window, cx);
}
pub(crate) fn register_lifecycle(cx: &mut App) {
    platform::register_lifecycle(cx);
}
pub(crate) fn activate_window(window: &mut Window, token: Option<&str>) {
    platform::activate_window(window, token);
}
/// Hide only this dashboard while keeping its sessions available to the tray.
pub(crate) fn hide_to_tray(window: &mut Window) {
    platform::hide_to_tray(window);
}
pub(crate) fn close_window(window: &mut Window, cx: &mut App) {
    if crate::desktop::tray::should_close(window, cx) {
        window.remove_window();
    }
}
