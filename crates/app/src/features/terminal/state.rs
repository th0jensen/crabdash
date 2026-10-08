//! Ghostty emulation and session state; rendering consumes snapshots from this module.
use super::{sizing::EmulatorSize, view::QuakeTerminalStatus};
use crate::components::terminal_input::TerminalInput;
use gpui::*;
use libghostty_vt::{
    RenderState, Terminal, TerminalOptions,
    key::{Encoder, Event, Key},
    render::{CellIterator, RowIterator},
    terminal::{Mode, ScrollViewport},
};
use machines::terminal::{TerminalController, TerminalSize};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use uuid::Uuid;
/// A complete native cell grapheme; spacer tails belong to their wide head.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalCluster {
    pub text: SharedString,
    pub column: u16,
    pub columns: u16,
}

/// A contiguous run of identically-styled text within a terminal row.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalSpan {
    pub text: SharedString,
    pub fg: Option<Rgba>,
    pub bg: Option<Rgba>,
    pub bold: bool,
    pub columns: u16,
    pub selected: bool,
    pub clusters: Vec<TerminalCluster>,
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
    pub(super) terminal: Terminal<'static, 'static>,
    pub(super) selection: super::selection::State,
    render_state: RenderState<'static>,
    pub rendered: RenderedTerminal,
    pub cursor: Option<TerminalCursor>,
    pub scroll_handle: ScrollHandle,
    pub loaded: bool,
    pending_pty_writes: Rc<RefCell<Vec<Vec<u8>>>>,
    pub(super) follow_output: bool,
    wheel_remainder: f32,
}

impl TerminalState {
    pub fn new_log(columns: u16, rows: u16) -> anyhow::Result<Self> {
        let mut state = Self::new(columns, rows, 5000)?;
        // Log commands commonly emit bare line feeds rather than terminal CRLF.
        state.terminal.vt_write(b"\x1b[20h");
        Ok(state)
    }

    pub fn new_interactive(
        columns: u16,
        rows: u16,
        scrollback: u32,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> anyhow::Result<Self> {
        let mut state = Self::new(columns, rows, scrollback)?;
        // Set metrics before the first PTY output can query cell dimensions.
        state
            .terminal
            .resize(columns, rows, cell_width_px, cell_height_px)?;
        Ok(state)
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
        terminal.on_size(|terminal| {
            let columns = terminal.cols().ok()?;
            let rows = terminal.rows().ok()?;
            let cell_width = terminal.width_px().ok()?.checked_div(u32::from(columns))?;
            let cell_height = terminal.height_px().ok()?.checked_div(u32::from(rows))?;
            (cell_width > 0 && cell_height > 0).then_some(libghostty_vt::terminal::SizeReportSize {
                columns,
                rows,
                cell_width,
                cell_height,
            })
        })?;

        Ok(Self {
            terminal,
            selection: super::selection::State::default(),
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
        self.cancel_selection_gesture();
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

    pub(super) fn title(&self) -> String {
        self.terminal
            .title()
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_control())
            .take(256)
            .collect::<String>()
            .trim()
            .to_owned()
    }

    pub fn feed_string(&mut self, data: String) {
        self.feed(data.into_bytes());
    }

    /// Feed raw PTY bytes into Ghostty and update the rendered output.
    pub fn feed(&mut self, data: impl AsRef<[u8]>) {
        self.terminal.vt_write(data.as_ref());
        self.reconcile_selection_screen();
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

    pub(super) fn refresh_frame(&mut self) {
        if let Some((rendered, cursor)) = self.extract_rendered() {
            self.rendered = rendered;
            self.cursor = cursor;
        }
    }

    pub fn prepare_input(&mut self, bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.clear_selection()?;
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
        self.clear_selection()?;
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
            let selected = row.selection().ok().flatten();

            if let Ok(mut cell_iter) = cells_iter.update(row) {
                let mut current_text = String::new();
                let mut current_clusters = Vec::new();
                let mut current_fg: Option<Rgba> = None;
                let mut current_bg: Option<Rgba> = None;
                let mut current_bold = false;
                let mut current_columns = 0;
                let mut current_selected = false;
                let mut column = 0u16;

                while let Some(cell) = cell_iter.next() {
                    let cell_column = column;
                    column = column.saturating_add(1);
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
                    let selected = selected.as_ref().is_some_and(|range| {
                        cell_column <= range.end_x
                            && cell_column.saturating_add(columns - 1) >= range.start_x
                    });

                    if fg != current_fg
                        || bg != current_bg
                        || bold != current_bold
                        || selected != current_selected
                    {
                        push_span(
                            &mut line,
                            &current_text,
                            current_fg,
                            current_bg,
                            current_bold,
                            current_columns,
                            current_selected,
                            &current_clusters,
                        );
                        current_text.clear();
                        current_clusters.clear();
                        current_columns = 0;
                        current_fg = fg;
                        current_bg = bg;
                        current_bold = bold;
                        current_selected = selected;
                    }

                    current_columns += columns;
                    let graphemes = cell.graphemes().unwrap_or_default();
                    let start = current_text.len();
                    if graphemes.is_empty() {
                        current_text.push(' ');
                    } else {
                        current_text.extend(graphemes);
                    }
                    current_clusters.push(TerminalCluster {
                        text: current_text[start..].to_owned().into(),
                        column: cell_column,
                        columns,
                    });
                }
                push_span(
                    &mut line,
                    &current_text,
                    current_fg,
                    current_bg,
                    current_bold,
                    current_columns,
                    current_selected,
                    &current_clusters,
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
    selected: bool,
    clusters: &[TerminalCluster],
) {
    if !text.is_empty() {
        line.push(TerminalSpan {
            text: SharedString::from(text.to_owned()),
            fg,
            bg,
            bold,
            columns,
            selected,
            clusters: clusters.to_vec(),
        });
    }
}

fn trim_trailing_blank_cells(line: &mut Vec<TerminalSpan>) {
    while let Some(span) = line.last_mut() {
        if span.bg.is_some() || span.selected {
            break;
        }
        let trimmed_length = span.text.trim_end_matches(' ').len();
        if trimmed_length == 0 {
            line.pop();
            continue;
        }

        if trimmed_length != span.text.len() {
            while span
                .clusters
                .last()
                .is_some_and(|cell| cell.text.as_ref() == " ")
            {
                span.clusters.pop();
            }
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
    pub endpoint: String,
    pub(super) custom_name: Option<String>,
    pub(super) rename: Option<super::rename::Editor>,
    pub terminal: TerminalState,
    pub controller: Option<TerminalController>,
    pub size: TerminalSize,
    pub(super) emulator_size: EmulatorSize,
    pub status: QuakeTerminalStatus,
    pub input: Entity<TerminalInput>,
    pub(crate) _input_events: Subscription,
}

pub(crate) struct Drawer {
    pub(super) model: super::model::Model,
    pub(super) sessions: HashMap<Uuid, QuakeTerminal>,
}

impl Drawer {
    pub(crate) fn cancel_drag(&mut self) {
        self.model.drag_target = None;
    }

    pub(super) fn active(&self) -> Option<&QuakeTerminal> {
        self.model
            .layout
            .focused_tab()
            .and_then(|id| self.sessions.get(&id))
    }

    pub(crate) fn shutdown(&self) {
        for session in self.sessions.values() {
            if let Some(controller) = &session.controller
                && let Err(error) = controller.shutdown()
            {
                tracing::debug!(%error, "Failed to shut down drawer terminal");
            }
        }
    }
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
    fn osc_zero_and_two_titles_survive_every_stream_byte_boundary() -> anyhow::Result<()> {
        let title = "Build · 雪:~/src";
        for command in [0, 2] {
            for terminator in ["\x07", "\x1b\\"] {
                let sequence = format!("\x1b]{command};{title}{terminator}");
                for boundary in 0..=sequence.len() {
                    let mut state = TerminalState::new_interactive(30, 5, 200, 8, 20)?;
                    state.feed(&sequence.as_bytes()[..boundary]);
                    state.feed(&sequence.as_bytes()[boundary..]);
                    assert_eq!(state.title(), title, "OSC {command}, byte {boundary}");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn ordinary_output_and_icon_titles_preserve_the_current_window_title() -> anyhow::Result<()> {
        let mut state = TerminalState::new_interactive(30, 5, 200, 8, 20)?;
        state.feed(b"\x1b]2;Build\x07");
        state.feed("ordinary 雪 output\r\n");
        state.feed(b"\x1b]1;Icon only\x1b\\");
        assert_eq!(state.title(), "Build");
        assert!(text(&state).contains("ordinary 雪 output"));
        state.feed(b"\x1b]0;New window\x1b\\");
        assert_eq!(state.title(), "New window");
        Ok(())
    }

    #[test]
    fn interactive_cell_metrics_are_available_before_first_resize() {
        let mut state = TerminalState::new_interactive(30, 5, 200, 16, 40).unwrap();
        assert!(state.rendered.is_empty());
        assert_eq!(state.terminal.width_px().unwrap(), 480);
        assert_eq!(state.terminal.height_px().unwrap(), 200);
        state.feed("\x1b[16t");
        assert!(
            state
                .take_pty_writes()
                .iter()
                .any(|response| response == b"\x1b[6;40;16t")
        );
    }

    #[test]
    fn resize_notifications_are_pending_without_waiting_for_more_output() {
        let mut state = TerminalState::new_interactive(30, 5, 200, 8, 20).unwrap();
        state.feed("\x1b[?2048h");
        state.take_pty_writes();
        state.resize(30, 5, 16, 40).unwrap();
        assert!(
            state
                .take_pty_writes()
                .iter()
                .any(|response| response == b"\x1b[48;5;30;200;480t")
        );
    }

    #[test]
    fn osc_titles_are_unicode_bounded_and_sanitized_for_chrome() {
        let mut state = TerminalState::new_interactive(30, 5, 200, 8, 20).unwrap();
        assert!(state.title().is_empty());
        state.feed("\x1b]2;  build 雪  \x07");
        assert_eq!(state.title(), "build 雪");
        state.feed(format!("\x1b]2;{}\x07", "界".repeat(300)));
        assert_eq!(state.title().chars().count(), 256);
        assert!(
            state
                .title()
                .chars()
                .all(|character| !character.is_control())
        );
    }

    #[test]
    fn output_follows_live_screen_and_keeps_prompt_visible() {
        let mut state = TerminalState::new_interactive(30, 5, 200, 8, 20).unwrap();
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
        let mut state = TerminalState::new_interactive(30, 5, 200, 8, 20).unwrap();
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
        let mut state = TerminalState::new_interactive(30, 5, 200, 8, 20).unwrap();
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
        let mut state = TerminalState::new_interactive(30, 5, 200, 8, 20).unwrap();
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
        let mut state = TerminalState::new_interactive(20, 5, 200, 8, 20).unwrap();
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
