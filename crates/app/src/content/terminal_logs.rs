use gpui::prelude::*;
use gpui::*;
use libghostty_vt::{
    RenderState, Terminal, TerminalOptions,
    render::{CellIterator, RowIterator},
};

/// A contiguous run of identically-styled text within a terminal row.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalLogSpan {
    pub text: SharedString,
    pub fg: Option<Rgba>,
    pub bg: Option<Rgba>,
    pub bold: bool,
}

/// A rendered terminal frame: one entry per visible row, each row a list of spans.
pub type RenderedTerminalLogs = Vec<Vec<TerminalLogSpan>>;

/// Holds live terminal state for streaming ANSI-colored logs.
///
/// `Terminal` and `RenderState` are `!Send + !Sync` and must only be
/// accessed from the main GPUI thread (inside entity update callbacks).
pub struct TerminalLogState {
    terminal: Terminal<'static, 'static>,
    render_state: RenderState<'static>,
    pub rendered: RenderedTerminalLogs,
    pub scroll_handle: ScrollHandle,
    pub loaded: bool,
}

impl TerminalLogState {
    pub fn new(cols: u16) -> anyhow::Result<Self> {
        let mut terminal = Terminal::new(TerminalOptions {
            cols,
            rows: 50,
            max_scrollback: 5000,
        })?;
        // Enable Linefeed/Newline Mode (LNM): bare \n acts as CR+LF.
        // This keeps raw log output correctly column-aligned without
        // touching the bytes and without breaking ANSI cursor sequences.
        terminal.vt_write(b"\x1b[20h");
        Ok(Self {
            terminal,
            render_state: RenderState::new()?,
            rendered: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            loaded: false,
        })
    }

    pub fn feed_string(&mut self, data: String) {
        self.feed(data.into_bytes());
    }

    /// Feed raw log bytes into the terminal and update the rendered output.
    pub fn feed(&mut self, data: impl AsRef<[u8]>) {
        self.terminal.vt_write(data.as_ref());
        if let Some(rendered) = self.extract_rendered() {
            self.rendered = rendered;
        }
        self.loaded = true;
        self.scroll_handle.scroll_to_bottom();
    }

    fn extract_rendered(&mut self) -> Option<RenderedTerminalLogs> {
        let snapshot = self.render_state.update(&self.terminal).ok()?;

        let mut rows_iter = RowIterator::new().ok()?;
        let mut cells_iter = CellIterator::new().ok()?;
        let mut row_iter = rows_iter.update(&snapshot).ok()?;

        let mut result: RenderedTerminalLogs = Vec::new();

        while let Some(row) = row_iter.next() {
            let mut line: Vec<TerminalLogSpan> = Vec::new();

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
            }

            result.push(line);
        }

        while result.last().is_some_and(|row| row.is_empty()) {
            result.pop();
        }

        Some(result)
    }
}

fn push_span(
    line: &mut Vec<TerminalLogSpan>,
    text: &str,
    fg: Option<Rgba>,
    bg: Option<Rgba>,
    bold: bool,
) {
    let trimmed = text.trim_end();
    if !trimmed.is_empty() {
        line.push(TerminalLogSpan {
            text: SharedString::from(trimmed.to_owned()),
            fg,
            bg,
            bold,
        });
    }
}

fn rgb_to_rgba(r: u8, g: u8, b: u8) -> Rgba {
    rgba(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xFF)
}

const LOG_FONT_FAMILY: &str = "JetBrainsMono Nerd Font";
/// Approximate advance width of a JetBrainsMono glyph at `text_xs` (12 px).
const CHAR_WIDTH_PX: f32 = 7.2;
const LINE_HEIGHT_PX: f32 = 18.0;
const MAX_VIEWPORT_HEIGHT_PX: f32 = 300.0;

pub fn minimum_viewport_height() -> Pixels {
    px(LINE_HEIGHT_PX)
}

pub fn viewport_height(logs: &RenderedTerminalLogs) -> Pixels {
    px((logs.len().max(1) as f32 * LINE_HEIGHT_PX).min(MAX_VIEWPORT_HEIGHT_PX))
}

/// Render a parsed terminal frame as a column of styled text rows.
pub fn render_view(logs: &RenderedTerminalLogs) -> Div {
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
        .children(logs.iter().map(|line| {
            div()
                .flex()
                .when(line.is_empty(), |div| div.child(" "))
                .children(line.iter().map(|span| {
                    let fg = span.fg.unwrap_or_else(|| rgba(0xAEAEB2FF));
                    let div = div()
                        .text_xs()
                        .whitespace_nowrap()
                        .text_color(fg)
                        .when(span.bold, |div| div.font_weight(FontWeight::BOLD))
                        .child(span.text.clone());
                    match span.bg {
                        Some(bg) => div.bg(bg),
                        None => div,
                    }
                }))
        }))
}
