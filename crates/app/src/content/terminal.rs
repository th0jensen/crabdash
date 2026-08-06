use std::{cell::RefCell, rc::Rc};

use gpui::prelude::*;
use gpui::*;
use libghostty_vt::{
    RenderState, Terminal, TerminalOptions,
    render::{CellIterator, RowIterator},
};
use lucide_icons::Icon;
use machines::terminal::{TerminalController, TerminalSize};

use crate::{
    app::Crabdash,
    components::{common::lucide_icon, terminal_input::TerminalInput},
};

/// A contiguous run of identically-styled text within a terminal row.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalSpan {
    pub text: SharedString,
    pub fg: Option<Rgba>,
    pub bg: Option<Rgba>,
    pub bold: bool,
}

/// A rendered terminal frame: one entry per visible row, each row a list of spans.
pub type RenderedTerminal = Vec<Vec<TerminalSpan>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalCursor {
    pub column: u16,
    pub row: u16,
}

/// Holds live Ghostty VT state for streamed terminal output.
///
/// `Terminal` and `RenderState` are `!Send + !Sync` and must only be
/// accessed from the main GPUI thread (inside entity update callbacks).
pub struct TerminalState {
    terminal: Terminal<'static, 'static>,
    render_state: RenderState<'static>,
    pub rendered: RenderedTerminal,
    pub cursor: Option<TerminalCursor>,
    pub scroll_handle: ScrollHandle,
    pub loaded: bool,
    pending_pty_writes: Rc<RefCell<Vec<Vec<u8>>>>,
}

impl TerminalState {
    pub fn new_log(columns: u16) -> anyhow::Result<Self> {
        let mut state = Self::new(columns, 50)?;
        // Log commands commonly emit bare line feeds rather than terminal CRLF.
        state.terminal.vt_write(b"\x1b[20h");
        Ok(state)
    }

    pub fn new_interactive(columns: u16, rows: u16) -> anyhow::Result<Self> {
        Self::new(columns, rows)
    }

    fn new(columns: u16, rows: u16) -> anyhow::Result<Self> {
        let pending_pty_writes = Rc::new(RefCell::new(Vec::new()));
        let mut terminal = Terminal::new(TerminalOptions {
            cols: columns,
            rows,
            max_scrollback: 5000,
        })?;
        terminal.on_pty_write({
            let pending_pty_writes = pending_pty_writes.clone();
            move |_terminal, data| pending_pty_writes.borrow_mut().push(data.to_vec())
        })?;

        Ok(Self {
            terminal,
            render_state: RenderState::new()?,
            rendered: Vec::new(),
            cursor: None,
            scroll_handle: ScrollHandle::new(),
            loaded: false,
            pending_pty_writes,
        })
    }

    pub fn resize(
        &mut self,
        columns: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> anyhow::Result<()> {
        self.terminal
            .resize(columns, rows, cell_width_px, cell_height_px)?;
        if let Some((rendered, cursor)) = self.extract_rendered() {
            self.rendered = rendered;
            self.cursor = cursor;
        }
        Ok(())
    }

    pub fn take_pty_writes(&self) -> Vec<Vec<u8>> {
        self.pending_pty_writes.take()
    }

    pub fn feed_string(&mut self, data: String) {
        self.feed(data.into_bytes());
    }

    /// Feed raw PTY bytes into Ghostty and update the rendered output.
    pub fn feed(&mut self, data: impl AsRef<[u8]>) {
        self.terminal.vt_write(data.as_ref());
        if let Some((rendered, cursor)) = self.extract_rendered() {
            self.rendered = rendered;
            self.cursor = cursor;
        }
        self.loaded = true;
        self.scroll_handle.scroll_to_bottom();
    }

    fn extract_rendered(&mut self) -> Option<(RenderedTerminal, Option<TerminalCursor>)> {
        let snapshot = self.render_state.update(&self.terminal).ok()?;
        let cursor = snapshot
            .cursor_visible()
            .ok()
            .filter(|visible| *visible)
            .and_then(|_| snapshot.cursor_viewport().ok().flatten())
            .map(|cursor| TerminalCursor {
                column: cursor.x,
                row: cursor.y,
            });

        let mut rows_iter = RowIterator::new().ok()?;
        let mut cells_iter = CellIterator::new().ok()?;
        let mut row_iter = rows_iter.update(&snapshot).ok()?;

        let mut result: RenderedTerminal = Vec::new();

        while let Some(row) = row_iter.next() {
            let mut line: Vec<TerminalSpan> = Vec::new();

            if let Ok(mut cell_iter) = cells_iter.update(row) {
                let mut current_text = String::new();
                let mut current_fg: Option<Rgba> = None;
                let mut current_bg: Option<Rgba> = None;
                let mut current_bold = false;

                while let Some(cell) = cell_iter.next() {
                    let style = cell.style().unwrap_or_default();
                    let fg = cell
                        .fg_color()
                        .ok()
                        .flatten()
                        .map(|color| rgb_to_rgba(color.r, color.g, color.b));
                    let bg = cell
                        .bg_color()
                        .ok()
                        .flatten()
                        .map(|color| rgb_to_rgba(color.r, color.g, color.b));
                    let bold = style.bold;

                    if fg != current_fg || bg != current_bg || bold != current_bold {
                        push_span(
                            &mut line,
                            &current_text,
                            current_fg,
                            current_bg,
                            current_bold,
                        );
                        current_text.clear();
                        current_fg = fg;
                        current_bg = bg;
                        current_bold = bold;
                    }

                    let graphemes = cell.graphemes().unwrap_or_default();
                    if graphemes.is_empty() {
                        current_text.push(' ');
                    } else {
                        current_text.extend(graphemes);
                    }
                }
                push_span(
                    &mut line,
                    &current_text,
                    current_fg,
                    current_bg,
                    current_bold,
                );
                trim_trailing_blank_cells(&mut line);
            }

            result.push(line);
        }

        let minimum_rows = cursor
            .map(|cursor| usize::from(cursor.row) + 1)
            .unwrap_or_default();
        while result.len() > minimum_rows && result.last().is_some_and(|row| row.is_empty()) {
            result.pop();
        }

        Some((result, cursor))
    }
}

fn push_span(
    line: &mut Vec<TerminalSpan>,
    text: &str,
    fg: Option<Rgba>,
    bg: Option<Rgba>,
    bold: bool,
) {
    if !text.is_empty() {
        line.push(TerminalSpan {
            text: SharedString::from(text.to_owned()),
            fg,
            bg,
            bold,
        });
    }
}

fn trim_trailing_blank_cells(line: &mut Vec<TerminalSpan>) {
    while let Some(span) = line.last_mut() {
        let trimmed_length = span.text.trim_end_matches(' ').len();
        if trimmed_length == 0 {
            line.pop();
            continue;
        }

        if trimmed_length != span.text.len() {
            span.text = SharedString::from(span.text[..trimmed_length].to_owned());
        }
        break;
    }
}

fn rgb_to_rgba(r: u8, g: u8, b: u8) -> Rgba {
    rgba(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xFF)
}

const LOG_FONT_FAMILY: &str = "JetBrainsMono Nerd Font";
pub(crate) const INTERACTIVE_CELL_WIDTH_PX: f32 = 8.0;
pub(crate) const INTERACTIVE_CELL_HEIGHT_PX: f32 = 20.0;
// 16 rows × 20px exactly fills the quake panel: 380 − 34 header − 26 padding.
pub(crate) const INTERACTIVE_ROWS: u16 = 16;
const CHAR_WIDTH_PX: f32 = INTERACTIVE_CELL_WIDTH_PX;
const LINE_HEIGHT_PX: f32 = INTERACTIVE_CELL_HEIGHT_PX;
const MAX_VIEWPORT_HEIGHT_PX: f32 = 300.0;

pub fn minimum_viewport_height() -> Pixels {
    px(LINE_HEIGHT_PX)
}

pub fn viewport_height(logs: &RenderedTerminal) -> Pixels {
    px((logs.len().max(1) as f32 * LINE_HEIGHT_PX).min(MAX_VIEWPORT_HEIGHT_PX))
}

/// Render a Ghostty terminal frame as styled text rows.
pub fn render_view(logs: &RenderedTerminal) -> Div {
    render_view_with_cursor(logs, None)
}

fn render_view_with_cursor(logs: &RenderedTerminal, cursor: Option<TerminalCursor>) -> Div {
    let max_chars = logs
        .iter()
        .map(|line| {
            line.iter()
                .map(|span| span.text.chars().count())
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);

    div()
        .font_family(LOG_FONT_FAMILY)
        .min_w(px(max_chars as f32 * CHAR_WIDTH_PX))
        .flex()
        .flex_col()
        .items_start()
        .children(logs.iter().enumerate().map(|(row_index, line)| {
            div()
                .relative()
                .h(px(LINE_HEIGHT_PX))
                .flex()
                .items_center()
                .when(line.is_empty(), |div| div.child(" "))
                .children(line.iter().map(|span| {
                    let fg = span.fg.unwrap_or_else(|| rgba(0xAEAEB2FF));
                    let div = div()
                        .text_size(px(13.0))
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
                                .left(px(f32::from(cursor.column) * CHAR_WIDTH_PX))
                                .top(px(2.0))
                                .w(px(1.5))
                                .h(px(LINE_HEIGHT_PX - 4.0))
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

pub(crate) struct QuakeTerminal {
    pub machine_name: String,
    pub endpoint: String,
    pub terminal: TerminalState,
    pub controller: Option<TerminalController>,
    pub size: TerminalSize,
    pub status: QuakeTerminalStatus,
    pub input: Entity<TerminalInput>,
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
        .h(px(380.0))
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
        .child(
            div()
                .h(px(34.0))
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
                        .text_size(px(13.0))
                        .text_color(rgb(0xD4D4D4))
                        .child(lucide_icon(Icon::Terminal, 13.0))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("Terminal"))
                        .child(lucide_icon(Icon::ChevronRight, 10.0))
                        .child(quake.machine_name.clone())
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded(px(3.0))
                                .bg(rgb(0x222222))
                                .text_size(px(10.0))
                                .text_color(rgb(0x858585))
                                .child(quake.endpoint.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(10.0))
                                .text_color(status.color())
                                .child(div().size(px(7.0)).rounded_full().bg(status.color()))
                                .child(status.label()),
                        ),
                )
                .child(
                    div()
                        .id("close-quake-terminal")
                        .size(px(24.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0xAEAEB2))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x2A2D2E)).text_color(white()))
                        .child(lucide_icon(Icon::X, 14.0))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_quake_terminal(window, cx);
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
                .cursor_text()
                .when(!has_output, |this| {
                    this.child(
                        div()
                            .font_family(LOG_FONT_FAMILY)
                            .text_xs()
                            .text_color(rgb(0x6C6C70))
                            .child("Starting xterm-ghostty session…"),
                    )
                })
                .when(has_output, |this| {
                    this.child(render_view_with_cursor(
                        &quake.terminal.rendered,
                        quake.terminal.cursor,
                    ))
                })
                .child(quake.input.clone()),
        )
}
