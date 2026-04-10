use gpui::prelude::*;
use gpui::*;
use libghostty_vt::{
    RenderState, Terminal, TerminalOptions,
    render::{CellIterator, RowIterator},
};

/// A contiguous run of identically-styled text within a terminal row.
#[derive(Clone, Debug, PartialEq)]
pub struct LogSpan {
    pub text: String,
    pub fg: Option<Rgba>,
    pub bg: Option<Rgba>,
    pub bold: bool,
}

/// A rendered terminal frame: one entry per visible row, each row a list of spans.
pub type RenderedLogs = Vec<Vec<LogSpan>>;

/// Holds the live terminal state for a streaming container log.
///
/// `Terminal` and `RenderState` are `!Send + !Sync` and must only be
/// accessed from the main GPUI thread (inside entity update callbacks).
pub struct DockerLogState {
    terminal: Terminal<'static, 'static>,
    render_state: RenderState<'static>,
    pub rendered: RenderedLogs,
    pub scroll_handle: ScrollHandle,
    pub loaded: bool,
}

impl DockerLogState {
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
        let data = data.into_bytes();
        self.feed(&data);
    }

    /// Feed raw log bytes into the terminal and update the rendered output.
    pub fn feed(&mut self, data: &[u8]) {
        self.terminal.vt_write(data);
        if let Some(rendered) = self.extract_rendered() {
            self.rendered = rendered;
        }
        self.loaded = true;
        self.scroll_handle.scroll_to_bottom();
    }

    fn extract_rendered(&mut self) -> Option<RenderedLogs> {
        let snapshot = self.render_state.update(&self.terminal).ok()?;

        let mut rows_iter = RowIterator::new().ok()?;
        let mut cells_iter = CellIterator::new().ok()?;
        let mut row_iter = rows_iter.update(&snapshot).ok()?;

        let mut result: RenderedLogs = Vec::new();

        while let Some(row) = row_iter.next() {
            let mut line: Vec<LogSpan> = Vec::new();

            if let Ok(mut cell_iter) = cells_iter.update(row) {
                let mut cur_text = String::new();
                let mut cur_fg: Option<Rgba> = None;
                let mut cur_bg: Option<Rgba> = None;
                let mut cur_bold = false;

                while let Some(cell) = cell_iter.next() {
                    let graphemes = cell.graphemes().unwrap_or_default();
                    let text: String = if graphemes.is_empty() {
                        " ".to_string()
                    } else {
                        graphemes.into_iter().collect()
                    };

                    let style = cell.style().unwrap_or_default();
                    let fg = cell
                        .fg_color()
                        .ok()
                        .flatten()
                        .map(|c| rgb_to_rgba(c.r, c.g, c.b));
                    let bg = cell
                        .bg_color()
                        .ok()
                        .flatten()
                        .map(|c| rgb_to_rgba(c.r, c.g, c.b));
                    let bold = style.bold;

                    if fg == cur_fg && bg == cur_bg && bold == cur_bold {
                        cur_text.push_str(&text);
                    } else {
                        push_span(&mut line, &cur_text, cur_fg, cur_bg, cur_bold);
                        cur_text = text;
                        cur_fg = fg;
                        cur_bg = bg;
                        cur_bold = bold;
                    }
                }
                push_span(&mut line, &cur_text, cur_fg, cur_bg, cur_bold);
            }

            result.push(line);
        }

        while result.last().map_or(false, |r| r.is_empty()) {
            result.pop();
        }

        Some(result)
    }
}

fn push_span(line: &mut Vec<LogSpan>, text: &str, fg: Option<Rgba>, bg: Option<Rgba>, bold: bool) {
    let trimmed = text.trim_end();
    if !trimmed.is_empty() {
        line.push(LogSpan {
            text: trimmed.to_string(),
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

/// Render a parsed terminal frame as a column of styled text rows.
pub fn render_view(logs: &RenderedLogs) -> Div {
    let max_chars = logs
        .iter()
        .map(|line| line.iter().map(|s| s.text.chars().count()).sum::<usize>())
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
                .when(line.is_empty(), |d| d.child(" "))
                .children(line.iter().map(|span| {
                    let fg = span.fg.unwrap_or_else(|| rgba(0xAEAEB2FF));
                    let d = div()
                        .text_xs()
                        .whitespace_nowrap()
                        .text_color(fg)
                        .when(span.bold, |d| d.font_weight(FontWeight::BOLD))
                        .child(span.text.clone());
                    match span.bg {
                        Some(bg) => d.bg(bg),
                        None => d,
                    }
                }))
        }))
}
