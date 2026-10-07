use super::{TerminalCursor, TerminalSpan};
use gpui::prelude::*;
use gpui::*;
use libghostty_vt::terminal::ScrollViewport;
use lucide_icons::Icon;

use crate::components::style;
use crate::{app::Crabdash, components::common::lucide_icon};

const QUAKE_VERTICAL_PADDING_PX: f32 = 26.0;
const QUAKE_MIN_ROWS: u16 = 4;
const QUAKE_MAX_ROWS: u16 = 48;

pub(crate) fn cell_metrics(
    settings: &crate::features::preferences::Preferences,
    cx: &App,
) -> (f32, f32) {
    let font_id = cx
        .text_system()
        .resolve_font(&font(settings.terminal_font.clone()));
    let width = cx
        .text_system()
        .advance(font_id, px(settings.terminal_font_size), 'M')
        .map(|advance| f32::from(advance.width))
        .unwrap_or(settings.terminal_font_size * 0.6)
        .max(1.0);
    (
        width,
        (settings.terminal_font_size * settings.terminal_line_height).ceil(),
    )
}

pub(crate) fn quake_rows_for_height(height: Pixels, cell_height: f32, header_height: f32) -> u16 {
    let rows =
        ((f32::from(height) - header_height - QUAKE_VERTICAL_PADDING_PX) / cell_height).floor();
    rows.clamp(f32::from(QUAKE_MIN_ROWS), f32::from(QUAKE_MAX_ROWS)) as u16
}

pub(crate) fn quake_height_for_rows(rows: u16, cell_height: f32, header_height: f32) -> Pixels {
    px(header_height + QUAKE_VERTICAL_PADDING_PX + f32::from(rows) * cell_height)
}

/// Render a Ghostty terminal frame as styled text rows.
pub fn render_view(logs: &[Vec<TerminalSpan>], cx: &App) -> Div {
    render_view_with_cursor(logs, None, rgb(style::TEXT_PRIMARY), cx)
}

fn render_view_with_cursor(
    logs: &[Vec<TerminalSpan>],
    cursor: Option<TerminalCursor>,
    default_fg: Rgba,
    cx: &App,
) -> Div {
    let settings = crate::features::preferences::current(cx);
    let (char_width, line_height) = cell_metrics(&settings, cx);
    let max_chars = logs
        .iter()
        .map(|line| {
            line.iter()
                .map(|span| usize::from(span.columns))
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);

    div()
        .font_family(settings.terminal_font.clone())
        .min_w(px(max_chars as f32 * char_width))
        .flex()
        .flex_col()
        .flex_none()
        .items_start()
        .children(logs.iter().enumerate().map(|(row_index, line)| {
            div()
                .relative()
                .h(px(line_height))
                .flex_none()
                .text_size(px(settings.terminal_font_size))
                .line_height(px(line_height))
                .flex()
                .items_center()
                .when(line.is_empty(), |div| div.child(" "))
                .children(line.iter().map(|span| {
                    let fg = span.fg.unwrap_or(default_fg);
                    let div = div()
                        .text_size(px(settings.terminal_font_size))
                        .w(px(f32::from(span.columns) * char_width))
                        .flex_none()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(fg)
                        .when(span.bold, |div| div.font_weight(FontWeight::BOLD))
                        .child(span.text.clone());
                    match span.bg {
                        Some(bg) => div.bg(bg),
                        None => div,
                    }
                }))
                .when_some(
                    cursor.filter(|cursor| usize::from(cursor.row) == row_index),
                    |this, cursor| {
                        this.child(
                            div()
                                .absolute()
                                .left(px(f32::from(cursor.column) * char_width))
                                .top(px(2.0))
                                .w(px(1.5))
                                .h(px(line_height - 4.0))
                                .bg(rgb(0xD7D7D7)),
                        )
                    },
                )
        }))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuakeTerminalStatus {
    Connecting,
    Connected,
    Exited,
    Failed,
}

impl QuakeTerminalStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Exited => "Exited",
            Self::Failed => "Failed",
        }
    }

    fn color(self) -> Rgba {
        match self {
            Self::Connecting => rgb(0xFFD60A),
            Self::Connected => rgb(0x30D158),
            Self::Exited => rgb(0x8E8E93),
            Self::Failed => rgb(0xFF453A),
        }
    }
}

/// Marker for an in-progress quake panel resize drag.
pub(crate) struct QuakeResizeDrag;

/// Invisible drag view: resizing is driven by `on_drag_move`, nothing follows the cursor.
struct QuakeResizeDragView;

impl Render for QuakeResizeDragView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_0()
    }
}

pub(crate) fn render_quake(
    app: &Crabdash,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> impl IntoElement {
    let Some(quake) = app.active_quake_terminal() else {
        return div();
    };

    let status = quake.status;
    let focused = quake.input.focus_handle(cx).is_focused(window);
    let has_output = !quake.terminal.rendered.is_empty();

    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(app.quake_height)
        .bg(rgb(0x181818))
        .border_t_1()
        .border_color(if focused {
            rgb(0x484848)
        } else {
            rgb(0x303030)
        })
        .shadow_lg()
        .flex()
        .flex_col()
        .occlude()
        .on_drag_move::<QuakeResizeDrag>(cx.listener(
            |this, event: &DragMoveEvent<QuakeResizeDrag>, window, cx| {
                cx.set_active_drag_cursor_style(CursorStyle::ResizeUpDown, window);
                let viewport_height = window.viewport_size().height;
                let (_, cell_height) = cell_metrics(&this.preferences, cx);
                let min_height = quake_height_for_rows(
                    QUAKE_MIN_ROWS,
                    cell_height,
                    f32::from(window.rem_size()) * style::BAR / 16.0,
                );
                let height = (viewport_height - event.event.position.y)
                    .clamp(min_height, viewport_height - px(80.0));
                this.set_quake_height(height, window, cx);
            },
        ))
        .child(
            div()
                .h(gpui::rems(style::BAR / 16.0))
                .flex_none()
                .px(px(10.0))
                .bg(rgb(0x181818))
                .border_b_1()
                .border_color(rgb(0x2B2B2B))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(7.0))
                        .text_size(gpui::rems(style::TEXT / 16.0))
                        .text_color(rgb(0xD4D4D4))
                        .child(lucide_icon(Icon::Terminal, 13.0))
                        .child("Terminal")
                        .child(lucide_icon(Icon::ChevronRight, 10.0))
                        .child(quake.machine_name.clone())
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded(px(3.0))
                                .bg(rgb(0x222222))
                                .text_size(gpui::rems(style::META / 16.0))
                                .text_color(rgb(0x858585))
                                .child(quake.endpoint.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(gpui::rems(style::META / 16.0))
                                .text_color(status.color())
                                .child(div().size(px(7.0)).rounded_full().bg(status.color()))
                                .child(status.label()),
                        ),
                )
                .child(
                    div()
                        .id("close-quake-terminal")
                        .size(gpui::rems(style::CONTROL / 16.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0xAEAEB2))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x2A2D2E)).text_color(white()))
                        .child(lucide_icon(Icon::X, 14.0))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.set_quake_terminal_open(false, window, cx);
                        })),
                ),
        )
        .child(
            div()
                .id("quake-terminal-content")
                .relative()
                .flex_1()
                .min_h_0()
                .w_full()
                .px(px(10.0))
                .pt(px(16.0))
                .pb(px(10.0))
                .overflow_hidden()
                .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                    let (_, line_height) = cell_metrics(&this.preferences, cx);
                    let uuid = this.selected_machine().uuid;
                    if let Some(quake) = this.quake_terminals.get_mut(&uuid) {
                        quake.terminal.scroll_wheel(event.delta, line_height);
                        cx.notify();
                        cx.stop_propagation();
                    }
                }))
                .cursor_text()
                .when(!has_output, |this| {
                    this.child(
                        div()
                            .font_family(app.preferences.terminal_font.clone())
                            .text_size(gpui::rems(style::META / 16.0))
                            .text_color(rgb(0x6C6C70))
                            .child(format!(
                                "Starting {} session…",
                                app.preferences.terminal_type
                            )),
                    )
                })
                .when(has_output, |this| {
                    this.child(render_view_with_cursor(
                        &quake.terminal.rendered,
                        quake.terminal.cursor,
                        rgba(0xAEAEB2FF),
                        cx,
                    ))
                })
                .child(quake.input.clone())
                .when(!quake.terminal.following_output(), |this| {
                    this.child(
                        div()
                            .id("terminal-follow-output")
                            .absolute()
                            .right(px(12.0))
                            .bottom(px(8.0))
                            .px(px(10.0))
                            .py(px(5.0))
                            .rounded(px(4.0))
                            .bg(rgb(0x343434))
                            .text_size(gpui::rems(style::META / 16.0))
                            .text_color(rgb(0xD7D7D7))
                            .cursor_pointer()
                            .child("↓ Latest output")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, window, cx| {
                                let uuid = this.selected_machine().uuid;
                                if let Some(quake) = this.quake_terminals.get_mut(&uuid) {
                                    quake.terminal.scroll(ScrollViewport::Bottom);
                                    window.focus(&quake.input.focus_handle(cx));
                                    cx.notify();
                                }
                            })),
                    )
                }),
        )
        .child(
            div()
                .id("quake-resize-handle")
                .absolute()
                .top(px(-3.0))
                .left_0()
                .right_0()
                .h(px(7.0))
                .cursor(CursorStyle::ResizeUpDown)
                .on_drag(QuakeResizeDrag, |_, _, _, cx| {
                    cx.new(|_| QuakeResizeDragView)
                }),
        )
}
