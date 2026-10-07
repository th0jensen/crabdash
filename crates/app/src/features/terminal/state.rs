//! Ghostty emulation and session state; rendering consumes snapshots from this module.
use super::view::QuakeTerminalStatus;
use crate::components::terminal_input::TerminalInput;
use gpui::*;
use libghostty_vt::{
    RenderState, Terminal, TerminalOptions,
    key::{Encoder, Event, Key},
    render::{CellIterator, RowIterator},
    terminal::{Mode, ScrollViewport},
};
use machines::terminal::{TerminalController, TerminalSize};
use std::{cell::RefCell, rc::Rc};
/// A contiguous run of identically-styled text within a terminal row.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalSpan {
    pub text: SharedString,
    pub fg: Option<Rgba>,
    pub bg: Option<Rgba>,
    pub bold: bool,
    pub columns: u16,
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
    follow_output: bool,
    wheel_remainder: f32,
}

impl TerminalState {
    pub fn new_log(columns: u16, rows: u16) -> anyhow::Result<Self> {
        let mut state = Self::new(columns, rows, 5000)?;
        // Log commands commonly emit bare line feeds rather than terminal CRLF.
        state.terminal.vt_write(b"\x1b[20h");
        Ok(state)
    }

    pub fn new_interactive(columns: u16, rows: u16, scrollback: u32) -> anyhow::Result<Self> {
        Self::new(columns, rows, scrollback)
    }

    fn new(columns: u16, rows: u16, scrollback: u32) -> anyhow::Result<Self> {
        let pending_pty_writes = Rc::new(RefCell::new(Vec::new()));
        let mut terminal = Terminal::new(TerminalOptions {
            cols: columns,
            rows,
            max_scrollback: scrollback as usize,
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
            follow_output: true,
            wheel_remainder: 0.0,
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
        if self.follow_output {
            self.terminal.scroll_viewport(ScrollViewport::Bottom);
        }
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
        if self.follow_output {
            self.terminal.scroll_viewport(ScrollViewport::Bottom);
        }
        if let Some((rendered, cursor)) = self.extract_rendered() {
            self.rendered = rendered;
            self.cursor = cursor;
        }
        self.loaded = true;
        self.scroll_handle.scroll_to_bottom();
    }

    /// Scroll Ghostty's viewport, keeping output pinned only at the live screen.
    pub fn scroll(&mut self, scroll: ScrollViewport) {
        self.terminal.scroll_viewport(scroll);
        self.follow_output = self
            .terminal
            .scrollbar()
            .map(|bar| bar.offset.saturating_add(bar.len) >= bar.total)
            .unwrap_or(true);
        self.refresh_frame();
    }

    pub fn following_output(&self) -> bool {
        self.follow_output
    }

    pub fn scroll_wheel(&mut self, delta: ScrollDelta, line_height: f32) {
        // GPUI wheel deltas are positive upwards; Ghostty rows are negative upwards.
        self.wheel_remainder -= f32::from(delta.pixel_delta(px(line_height)).y) / line_height;
        let rows = self.wheel_remainder.trunc() as isize;
        self.wheel_remainder -= rows as f32;
        if rows != 0 {
            self.scroll(ScrollViewport::Delta(rows));
        }
    }

    fn refresh_frame(&mut self) {
        if let Some((rendered, cursor)) = self.extract_rendered() {
            self.rendered = rendered;
            self.cursor = cursor;
        }
    }

    pub fn prepare_input(&mut self, bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.scroll(ScrollViewport::Bottom);
        // Honour application cursor mode in full-screen programs.
        let key = match bytes {
            b"\x1b[A" => Some(Key::ArrowUp),
            b"\x1b[B" => Some(Key::ArrowDown),
            b"\x1b[C" => Some(Key::ArrowRight),
            b"\x1b[D" => Some(Key::ArrowLeft),
            b"\x1b[H" => Some(Key::Home),
            b"\x1b[F" => Some(Key::End),
            _ => None,
        };
        if let Some(key) = key {
            let mut encoder = Encoder::new()?;
            encoder.set_options_from_terminal(&self.terminal);
            let mut event = Event::new()?;
            event.set_key(key);
            let mut encoded = Vec::with_capacity(32);
            encoder.encode_to_vec(&event, &mut encoded)?;
            Ok(encoded)
        } else {
            Ok(bytes.to_vec())
        }
    }

    pub fn prepare_paste(&mut self, text: &str) -> anyhow::Result<Vec<u8>> {
        self.scroll(ScrollViewport::Bottom);
        let mut data = text.as_bytes().to_vec();
        let mut encoded = vec![0; data.len() + 12];
        let count = libghostty_vt::paste::encode(
            &mut data,
            self.terminal.mode(Mode::BRACKETED_PASTE)?,
            &mut encoded,
        )?;
        encoded.truncate(count);
        Ok(encoded)
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
                let mut current_columns = 0;

                while let Some(cell) = cell_iter.next() {
                    let wide = cell.raw_cell().ok().and_then(|cell| cell.wide().ok());
                    if wide == Some(libghostty_vt::screen::CellWide::SpacerTail) {
                        continue;
                    }
                    let columns = if wide == Some(libghostty_vt::screen::CellWide::Wide) {
                        2
                    } else {
                        1
                    };
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
                            current_columns,
                        );
                        current_text.clear();
                        current_columns = 0;
                        current_fg = fg;
                        current_bg = bg;
                        current_bold = bold;
                    }

                    current_columns += columns;
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
                    current_columns,
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
    columns: u16,
) {
    if !text.is_empty() {
        line.push(TerminalSpan {
            text: SharedString::from(text.to_owned()),
            fg,
            bg,
            bold,
            columns,
        });
    }
}

fn trim_trailing_blank_cells(line: &mut Vec<TerminalSpan>) {
    while let Some(span) = line.last_mut() {
        if span.bg.is_some() {
            break;
        }
        let trimmed_length = span.text.trim_end_matches(' ').len();
        if trimmed_length == 0 {
            line.pop();
            continue;
        }

        if trimmed_length != span.text.len() {
            span.columns = span
                .columns
                .saturating_sub((span.text.len() - trimmed_length) as u16);
            span.text = SharedString::from(span.text[..trimmed_length].to_owned());
        }
        break;
    }
}

fn rgb_to_rgba(r: u8, g: u8, b: u8) -> Rgba {
    rgba(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xFF)
}

pub(crate) struct QuakeTerminal {
    pub machine_name: String,
    pub endpoint: String,
    pub terminal: TerminalState,
    pub controller: Option<TerminalController>,
    pub size: TerminalSize,
    pub status: QuakeTerminalStatus,
    pub input: Entity<TerminalInput>,
    pub(crate) _input_events: Subscription,
}

#[cfg(test)]
mod tests {
    use super::TerminalState;
    use libghostty_vt::terminal::ScrollViewport;
    use std::prelude::v1::test;

    fn text(state: &TerminalState) -> String {
        state
            .rendered
            .iter()
            .map(|row| {
                row.iter()
                    .map(|span| span.text.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn output_follows_live_screen_and_keeps_prompt_visible() {
        let mut state = TerminalState::new_interactive(30, 5, 200).unwrap();
        for index in 1..=100 {
            state.feed(format!("line-{index}\r\n"));
        }
        state.feed("prompt> ");
        assert!(text(&state).contains("line-100"));
        assert!(text(&state).ends_with("prompt>"));
        assert_eq!(state.cursor.unwrap().row, 4);
        assert!(state.following_output());
    }

    #[test]
    fn reading_history_stays_put_until_returning_to_prompt() {
        let mut state = TerminalState::new_interactive(30, 5, 200).unwrap();
        for index in 1..=30 {
            state.feed(format!("line-{index}\r\n"));
        }
        state.scroll(ScrollViewport::Delta(-8));
        assert!(!state.following_output());
        let previous = text(&state);
        state.feed("new-output\r\n");
        assert_eq!(text(&state), previous);
        assert!(state.cursor.is_none());
        assert_eq!(state.prepare_input(b"a").unwrap(), b"a");
        assert!(state.following_output());
        assert!(text(&state).contains("new-output"));
        state.scroll(ScrollViewport::Top);
        assert!(text(&state).starts_with("line-1"));
        state.scroll(ScrollViewport::Bottom);
        state.feed("latest\r\n");
        assert!(text(&state).contains("latest"));
    }

    #[test]
    fn resize_and_alternate_screen_keep_live_cursor_visible() {
        let mut state = TerminalState::new_interactive(30, 5, 200).unwrap();
        for _ in 0..40 {
            state.feed("output\r\n");
        }
        state.resize(40, 8, 8, 20).unwrap();
        state.feed("prompt> ");
        assert!(text(&state).contains("prompt>"));
        assert!(state.cursor.unwrap().row < 8);
        state.feed("\x1b[?1049h\x1b[2J\x1b[Hfull-screen");
        assert!(text(&state).starts_with("full-screen"));
        state.scroll(ScrollViewport::Top);
        assert!(state.following_output());
        state.feed("\x1b[?1049l");
        assert!(text(&state).contains("prompt>"));
    }

    #[test]
    fn cursor_keys_and_paste_follow_terminal_modes() {
        let mut state = TerminalState::new_interactive(30, 5, 200).unwrap();
        assert_eq!(state.prepare_input(b"\x1b[A").unwrap(), b"\x1b[A");
        state.feed("\x1b[?1h");
        assert_eq!(state.prepare_input(b"\x1b[A").unwrap(), b"\x1bOA");
        assert_eq!(state.prepare_paste("a\nb").unwrap(), b"a\rb");
        state.feed("\x1b[?2004h");
        assert_eq!(
            state.prepare_paste("a\nb").unwrap(),
            b"\x1b[200~a\nb\x1b[201~"
        );
    }

    #[test]
    fn unicode_cells_and_coloured_blank_background_keep_grid_width() {
        let mut state = TerminalState::new_interactive(20, 5, 200).unwrap();
        state.feed("A界e\u{301}B");
        assert_eq!(state.cursor.unwrap().column, 5);
        assert_eq!(
            state.rendered[0]
                .iter()
                .map(|span| span.columns)
                .sum::<u16>(),
            5
        );
        assert_eq!(text(&state), "A界e\u{301}B");
        state.feed("\r\n\x1b[41m   \x1b[0m");
        assert!(
            state.rendered[1]
                .iter()
                .any(|span| span.bg.is_some() && span.columns == 3)
        );
    }
}
