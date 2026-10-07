//! GPUI caption hit areas retain native Windows snapping and system commands.
use crate::components::{
    common::{control_tooltip, lucide_icon},
    style,
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IsIconic, IsZoomed, SW_HIDE, SW_MAXIMIZE, SW_RESTORE, SW_SHOW, ShowWindowAsync,
};

pub(super) const LEADING_PADDING: f32 = 0.0;
pub(super) fn configure(_: &mut WindowOptions) {}
pub(super) fn prepare(window: &mut Window, cx: &mut App) {
    window.on_window_should_close(cx, crate::desktop::tray::should_close);
}
pub(super) fn register_lifecycle(cx: &mut App) {
    cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}
pub(super) fn activate_window(window: &mut Window, _: Option<&str>) {
    if let Ok(handle) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            unsafe {
                let hwnd = handle.hwnd.get() as _;
                let command = if IsIconic(hwnd) != 0 {
                    SW_RESTORE
                } else {
                    SW_SHOW
                };
                // Post the transition to the owning window's queue. A synchronous
                // restore can deliver WM_SIZE while GPUI still holds its App borrow.
                if ShowWindowAsync(hwnd, command) == 0 {
                    tracing::warn!(
                        "Unable to restore the native Windows window from the tray: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
        }
    }
    window.activate_window();
}
pub(super) fn zoom(window: &mut Window) {
    if let Ok(handle) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            let hwnd = handle.hwnd.get() as _;
            // GPUI's Windows zoom currently only maximizes. Match the caption
            // button's native toggle and queue it without reentering GPUI's
            // active input callback through synchronous WM_SIZE delivery.
            unsafe {
                let command = if IsZoomed(hwnd) != 0 {
                    SW_RESTORE
                } else {
                    SW_MAXIMIZE
                };
                if ShowWindowAsync(hwnd, command) == 0 {
                    tracing::warn!(
                        "Unable to change the native Windows window state: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
            return;
        }
    }
    window.zoom_window();
}
pub(super) fn controls(window: &Window) -> Div {
    div()
        .flex_none()
        .ml(px(8.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(control(
            "window-minimize",
            Icon::Minus,
            WindowControlArea::Min,
            "Minimize · Ctrl+M",
            |_, window, _| window.minimize_window(),
        ))
        .child(control(
            "window-maximize",
            if window.is_maximized() {
                Icon::Copy
            } else {
                Icon::Square
            },
            WindowControlArea::Max,
            "Maximize / restore · Ctrl+Shift+M",
            |_, window, _| super::zoom(window),
        ))
        .child(control(
            "window-close",
            Icon::X,
            WindowControlArea::Close,
            "Close window · Ctrl+W",
            |_, window, cx| super::close_window(window, cx),
        ))
}
fn control(
    id: &'static str,
    icon: Icon,
    area: WindowControlArea,
    tooltip: &'static str,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .tooltip(move |_, cx| control_tooltip(tooltip, cx))
        .size(gpui::rems(style::CHROME_CONTROL / 16.0))
        .flex_none()
        .rounded(px(style::RADIUS))
        .window_control_area(area)
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(style::TEXT_MUTED))
        .cursor_pointer()
        .hover(move |this| {
            this.bg(if area == WindowControlArea::Close {
                rgb(0xB72E38)
            } else {
                rgb(style::SURFACE_HOVER)
            })
            .text_color(white())
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(lucide_icon(icon, style::ICON))
        .on_click(click)
}
// GPUI's Windows WM_NCHITTEST already provides native resize edges.
pub(crate) fn resize_handles(_: &Window) -> Div {
    div().absolute().inset_0().size_full()
}

pub(super) fn hide_to_tray(window: &mut Window) {
    if let Ok(handle) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            // Closing-to-tray runs inside GPUI's close callback; defer native
            // window messages until that callback releases its borrowed state.
            if unsafe { ShowWindowAsync(handle.hwnd.get() as _, SW_HIDE) } != 0 {
                return;
            }
            tracing::warn!(
                "Unable to hide the native Windows window to the tray: {}",
                std::io::Error::last_os_error()
            );
        }
    }
    window.minimize_window();
}
