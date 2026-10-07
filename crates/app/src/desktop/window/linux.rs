use crate::components::{
    common::{control_tooltip, lucide_icon},
    style,
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
fn window_button(
    id: &'static str,
    icon: Icon,
    area: WindowControlArea,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .tooltip(move |_, cx| {
            control_tooltip(
                match id {
                    "window-minimize" => "Minimize · Ctrl+M",
                    "window-maximize" => "Maximize / restore · Ctrl+Shift+M",
                    _ => "Close window · Ctrl+W",
                },
                cx,
            )
        })
        .size(gpui::rems(style::CHROME_CONTROL / 16.0))
        .flex_none()
        .rounded(px(style::RADIUS))
        .window_control_area(area)
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(0xB8B8B8))
        .cursor_pointer()
        .hover(move |this| {
            this.bg(if area == WindowControlArea::Close {
                rgb(0xB72E38)
            } else {
                rgb(0x343434)
            })
            .text_color(white())
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(lucide_icon(icon, 14.0))
        .on_click(handler)
}

pub(super) fn controls(window: &Window) -> Div {
    if !matches!(window.window_decorations(), Decorations::Client { .. }) {
        return div();
    }
    let controls = window.window_controls();
    div()
        .ml(px(8.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .when(controls.minimize, |this| {
            this.child(window_button(
                "window-minimize",
                Icon::Minus,
                WindowControlArea::Min,
                |_, window, cx| super::minimize(window, cx),
            ))
        })
        .when(controls.maximize, |this| {
            this.child(window_button(
                "window-maximize",
                if window.is_maximized() {
                    Icon::Copy
                } else {
                    Icon::Square
                },
                WindowControlArea::Max,
                |_, window, _| super::zoom(window),
            ))
        })
        .child(window_button(
            "window-close",
            Icon::X,
            WindowControlArea::Close,
            |_, window, cx| super::close_window(window, cx),
        ))
}

/// Resize handles are needed when the compositor delegates decorations to GPUI.
pub(crate) fn resize_handles(window: &Window) -> Div {
    let mut container = div().absolute().inset_0().size_full();
    if !matches!(window.window_decorations(), Decorations::Client { .. })
        || window.is_maximized()
        || window.is_fullscreen()
    {
        return container;
    }
    let size = window.viewport_size();
    for (name, edge, x, y, width, height, cursor) in [
        (
            "n",
            ResizeEdge::Top,
            px(6.0),
            px(0.0),
            size.width - px(12.0),
            px(4.0),
            CursorStyle::ResizeUpDown,
        ),
        (
            "s",
            ResizeEdge::Bottom,
            px(6.0),
            size.height - px(4.0),
            size.width - px(12.0),
            px(4.0),
            CursorStyle::ResizeUpDown,
        ),
        (
            "w",
            ResizeEdge::Left,
            px(0.0),
            px(6.0),
            px(4.0),
            size.height - px(12.0),
            CursorStyle::ResizeLeftRight,
        ),
        (
            "e",
            ResizeEdge::Right,
            size.width - px(4.0),
            px(6.0),
            px(4.0),
            size.height - px(12.0),
            CursorStyle::ResizeLeftRight,
        ),
        (
            "nw",
            ResizeEdge::TopLeft,
            px(0.0),
            px(0.0),
            px(6.0),
            px(6.0),
            CursorStyle::ResizeUpLeftDownRight,
        ),
        (
            "ne",
            ResizeEdge::TopRight,
            size.width - px(6.0),
            px(0.0),
            px(6.0),
            px(6.0),
            CursorStyle::ResizeUpRightDownLeft,
        ),
        (
            "sw",
            ResizeEdge::BottomLeft,
            px(0.0),
            size.height - px(6.0),
            px(6.0),
            px(6.0),
            CursorStyle::ResizeUpRightDownLeft,
        ),
        (
            "se",
            ResizeEdge::BottomRight,
            size.width - px(6.0),
            size.height - px(6.0),
            px(6.0),
            px(6.0),
            CursorStyle::ResizeUpLeftDownRight,
        ),
    ] {
        container = container.child(
            div()
                .id(SharedString::from(format!("window-resize-{name}")))
                .absolute()
                .left(x)
                .top(y)
                .w(width)
                .h(height)
                .cursor(cursor)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.start_window_resize(edge);
                    cx.stop_propagation();
                }),
        );
    }
    container
}
pub(super) fn activate_window(window: &mut Window, token: Option<&str>) {
    if let Some(token) = token {
        match super::activation::activate(window, token) {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "Unable to use the tray activation token"),
        }
    }
    window.activate_window();
}
pub(super) fn zoom(window: &mut Window) {
    window.zoom_window();
}

pub(super) fn configure(options: &mut WindowOptions) {
    options.window_decorations = Some(WindowDecorations::Client);
}
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
pub(super) const LEADING_PADDING: f32 = 0.0;

pub(super) fn hide_to_tray(window: &mut Window) {
    window.minimize_window();
}

pub(super) fn is_visible(_: &Window) -> Option<bool> {
    // xdg-shell v5 has no minimized-state notification. The shared visibility
    // module tracks our minimize/tray actions and restores on activation.
    None
}
