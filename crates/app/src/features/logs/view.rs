use crate::{
    app::Crabdash,
    components::{
        common::{control_tooltip, lucide_icon},
        style,
    },
    features::terminal::{self, TerminalState},
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;

/// Shared with variable-height lists so offscreen log panels invalidate their
/// cached height using the same trimmed content and cap as their presentation.
pub(crate) fn content_height(state: Option<&TerminalState>, line_height: f32) -> Pixels {
    let rows = state.map(|state| state.rendered.as_slice()).unwrap_or(&[]);
    let end = rows
        .iter()
        .rposition(|row| !row.is_empty())
        .map_or(0, |i| i + 1);
    px((end.max(1) as f32 * line_height).min(300.0))
}

pub(crate) fn render(
    id: impl Into<SharedString>,
    state: Option<&TerminalState>,
    cx: &mut Context<Crabdash>,
) -> Div {
    let id = id.into();
    let loading = state.is_none_or(|state| !state.loaded);
    // The VT cursor can leave a final empty row even for empty log output.
    let rows = state.map(|state| state.rendered.as_slice()).unwrap_or(&[]);
    let end = rows
        .iter()
        .rposition(|row| !row.is_empty())
        .map_or(0, |i| i + 1);
    let rows = &rows[..end];
    let scroll_handle = state
        .map(|state| state.scroll_handle.clone())
        .unwrap_or_default();
    let wheel_handle = scroll_handle.clone();
    let line_height = terminal::cell_metrics(&crate::features::preferences::current(cx), cx).1;
    let height = content_height(state, line_height);

    let content = if loading || rows.is_empty() {
        div()
            .text_size(rems(style::META / 16.0))
            .text_color(rgb(style::TEXT_MUTED))
            .child(if loading {
                "Loading logs…"
            } else {
                "No log entries."
            })
    } else {
        terminal::render_view(rows, cx)
    };

    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(10.0))
        .bg(rgb(style::SURFACE))
        .border_b_1()
        .border_color(rgb(style::BORDER))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(rems(style::CONTROL / 16.0))
                .text_size(rems(style::META / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child(lucide_icon(Icon::FileText, style::ICON))
                .child(div().flex_1().child("Recent logs"))
                .when(!loading && !rows.is_empty(), |this| {
                    let text = rows
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|span| span.text.as_ref())
                                .collect::<String>()
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    this.child(format!(
                        "{} {}",
                        rows.len(),
                        if rows.len() == 1 { "line" } else { "lines" }
                    ))
                    .child(
                        div()
                            .id(SharedString::from(format!("{id}-copy")))
                            .size(rems(style::CONTROL / 16.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(style::RADIUS))
                            .cursor_pointer()
                            .hover(|s| {
                                s.bg(rgb(style::SURFACE_HOVER))
                                    .text_color(rgb(style::TEXT_SELECTED))
                            })
                            .tooltip(|_, cx| control_tooltip("Copy logs", cx))
                            .child(lucide_icon(Icon::Copy, style::ICON))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                            }),
                    )
                }),
        )
        .child(
            div()
                .id(id)
                .w_full()
                .min_w_0()
                .h(height)
                .track_scroll(&scroll_handle)
                .overflow_scroll()
                .on_scroll_wheel(cx.listener(move |_, event: &ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(window.line_height());
                    let current = wheel_handle.offset();
                    let max = wheel_handle.max_offset();
                    wheel_handle.set_offset(point(
                        (current.x + delta.x).max(-max.width).min(px(0.0)),
                        (current.y + delta.y).max(-max.height).min(px(0.0)),
                    ));
                    cx.notify();
                    cx.stop_propagation();
                }))
                .child(content),
        )
}
