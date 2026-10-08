//! These exercise the bundled VT parser and tracked native selections, not display strings.
use super::Grid;
use crate::features::terminal::TerminalState;
use anyhow::{Context as _, Result};
use gpui::{Bounds, point, px, size};
use libghostty_vt::{
    selection::{
        Selection,
        gesture::{Autoscroll, Geometry},
    },
    terminal::{Point, PointCoordinate, ScrollViewport},
};

fn terminal(columns: u16, rows: u16) -> Result<TerminalState> {
    TerminalState::new_interactive(columns, rows, 1000, 8, 18)
}

fn cell(x: u16, y: u32) -> PointCoordinate {
    PointCoordinate { x, y }
}

fn install(state: &mut TerminalState, start: PointCoordinate, end: PointCoordinate) -> Result<()> {
    {
        let start = state.terminal.grid_ref(Point::Viewport(start))?;
        let end = state.terminal.grid_ref(Point::Viewport(end))?;
        let selection = Selection::new(start, end, false);
        state.terminal.set_selection(Some(&selection))?;
    }
    state.refresh_frame();
    Ok(())
}

fn geometry(columns: u16, rows: u16) -> Geometry {
    Geometry {
        columns: columns.into(),
        cell_width: 100,
        padding_left: 0,
        screen_height: u32::from(rows) * 18,
    }
}

#[test]
fn cell_drag_forward_reverse_and_threshold_use_native_gesture_rules() -> Result<()> {
    let mut state = terminal(32, 4)?;
    state.feed(b"alpha beta gamma");
    assert!(!state.has_selection());
    state.selection_press(cell(0, 0), 10.0, 1.0, 1)?;
    state.selection_drag(cell(0, 0), 30.0, 1.0, false, geometry(32, 4))?;
    assert!(!state.has_selection());
    state.selection_drag(cell(0, 0), 90.0, 1.0, false, geometry(32, 4))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("a"));
    state.selection_drag(cell(4, 0), 490.0, 1.0, false, geometry(32, 4))?;
    state.selection_release(Some(cell(4, 0)))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("alpha"));
    state.selection_press(cell(4, 0), 490.0, 1.0, 1)?;
    state.selection_drag(cell(0, 0), 10.0, 1.0, false, geometry(32, 4))?;
    state.selection_release(None)?;
    assert_eq!(state.selected_text()?.as_deref(), Some("alpha"));
    assert!(state.take_pty_writes().is_empty());
    Ok(())
}

#[test]
fn native_word_and_logical_line_clicks_include_soft_wrapped_content() -> Result<()> {
    let mut state = terminal(8, 4)?;
    state.feed(b"alpha beta gamma");
    state.selection_press(cell(6, 0), 650.0, 1.0, 2)?;
    state.selection_release(None)?;
    assert_eq!(state.selected_text()?.as_deref(), Some("beta"));
    state.selection_press(cell(1, 0), 150.0, 1.0, 3)?;
    state.selection_release(None)?;
    assert_eq!(state.selected_text()?.as_deref(), Some("alpha beta gamma"));
    Ok(())
}

#[test]
fn copied_graphemes_and_wide_halves_match_highlighted_cell_extents() -> Result<()> {
    let mut state = terminal(32, 4)?;
    state.feed("\x1b[31mA界e\u{301} 👩‍💻".as_bytes());
    // The native VT grid decides the ZWJ sequence's width in the current mode.
    let final_column = state.terminal.cursor_x()?;
    state.feed(b"Z\x1b[0m");
    assert_eq!(state.terminal.cursor_x()?, final_column + 1);
    install(&mut state, cell(0, 0), cell(final_column, 0))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("A界e\u{301} 👩‍💻Z"));
    install(&mut state, cell(final_column, 0), cell(final_column, 0))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("Z"));
    let mut column = 0;
    let mut selected_spans = 0;
    for span in &state.rendered[0] {
        if span.selected {
            selected_spans += 1;
            assert_eq!(column, final_column);
            assert_eq!(span.columns, 1);
            assert_eq!(span.text.as_ref(), "Z");
        }
        column += span.columns;
    }
    assert_eq!(selected_spans, 1);
    for half in [1, 2] {
        install(&mut state, cell(half, 0), cell(half, 0))?;
        assert_eq!(state.selected_text()?.as_deref(), Some("界"));
        let selected = state.rendered[0]
            .iter()
            .filter(|span| span.selected)
            .collect::<Vec<_>>();
        assert_eq!(selected.iter().map(|span| span.columns).sum::<u16>(), 2);
        assert_eq!(
            selected
                .iter()
                .map(|span| span.text.as_ref())
                .collect::<String>(),
            "界"
        );
    }
    install(&mut state, cell(3, 0), cell(3, 0))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("e\u{301}"));
    Ok(())
}

#[test]
fn copy_joins_soft_wraps_but_preserves_real_newlines_and_trims_trailing_spaces() -> Result<()> {
    let mut state = terminal(5, 5)?;
    state.feed(b"abcdefgh\r\nnext  ");
    install(&mut state, cell(0, 0), cell(0, 3))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("abcdefgh\nnext"));
    Ok(())
}

#[test]
fn whitespace_selection_and_selected_blank_rows_are_retained() -> Result<()> {
    let mut state = terminal(12, 4)?;
    state.feed(b"value   \r\n\r\n");
    install(&mut state, cell(5, 0), cell(7, 0))?;
    assert!(state.has_selection());
    assert!(state.selected_text()?.is_some());
    assert_eq!(
        state.rendered[0]
            .iter()
            .filter(|span| span.selected)
            .map(|span| span.columns)
            .sum::<u16>(),
        3
    );
    install(&mut state, cell(0, 3), cell(2, 3))?;
    assert!(state.has_selection());
    assert_eq!(state.rendered.len(), 4);
    assert!(
        state.rendered[3]
            .iter()
            .any(|span| span.selected && span.columns == 3)
    );
    Ok(())
}

#[test]
fn offscreen_selection_survives_output_and_reflow_without_snapshot_reuse() -> Result<()> {
    let mut state = terminal(20, 3)?;
    for index in 0..12 {
        state.feed(format!("line-{index}\r\n"));
    }
    state.scroll(ScrollViewport::Top);
    install(&mut state, cell(0, 0), cell(5, 0))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("line-0"));
    state.scroll(ScrollViewport::Bottom);
    assert!(state.has_selection());
    assert!(!state.rendered.iter().flatten().any(|span| span.selected));
    state.feed(b"later output\r\n");
    state.resize(8, 4, 8, 18)?;
    assert_eq!(state.selected_text()?.as_deref(), Some("line-0"));
    assert!(state.has_selection());
    Ok(())
}

#[test]
fn screen_switch_and_pruning_never_copy_stale_snapshot_content() -> Result<()> {
    let mut state = TerminalState::new_interactive(20, 3, 2, 8, 18)?;
    state.feed(b"selected-primary");
    install(&mut state, cell(0, 0), cell(7, 0))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("selected"));
    let primary_cursor = (state.terminal.cursor_x()?, state.terminal.cursor_y()?);
    state.feed(b"\x1b[?1049h");
    // DEC 1049 copies the current cursor into the newly cleared alternate screen.
    assert_eq!(
        (state.terminal.cursor_x()?, state.terminal.cursor_y()?),
        primary_cursor
    );
    assert!(!state.has_selection());
    assert_eq!(state.selected_text()?, None);
    state.feed(b"\x1b[Halt");
    assert_eq!(
        (state.terminal.cursor_x()?, state.terminal.cursor_y()?),
        (3, 0)
    );
    install(&mut state, cell(0, 0), cell(2, 0))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("alt"));
    state.feed(b"\x1b[?1049l");
    assert_eq!(
        (state.terminal.cursor_x()?, state.terminal.cursor_y()?),
        primary_cursor
    );
    // Switching screens clears the destination's prior selection, even primary.
    assert!(!state.has_selection());
    assert_eq!(state.selected_text()?, None);
    install(&mut state, cell(0, 0), cell(7, 0))?;
    assert_eq!(state.selected_text()?.as_deref(), Some("selected"));
    // Native history limits retain at least one complete page, even for two lines.
    let initial_rows = state.terminal.total_rows()?;
    let unused_active_rows = usize::from(
        state
            .terminal
            .rows()?
            .saturating_sub(state.terminal.cursor_y()? + 1),
    );
    let mut emitted_rows: usize = 0;
    let mut pruned = false;
    while emitted_rows < 4096 {
        let mut output = String::new();
        for index in emitted_rows..emitted_rows + 256 {
            output.push_str(&format!("\r\nreplacement-{index}"));
        }
        state.feed(output);
        emitted_rows += 256;
        let unpruned_rows = initial_rows + emitted_rows.saturating_sub(unused_active_rows);
        if state.terminal.total_rows()? < unpruned_rows {
            pruned = true;
            break;
        }
    }
    assert!(
        pruned,
        "bounded fixture must evict the original history page"
    );
    assert!(
        state.terminal.total_rows()?
            < initial_rows + emitted_rows.saturating_sub(unused_active_rows)
    );
    state.scroll(ScrollViewport::Top);
    let oldest_row = state.rendered[0]
        .iter()
        .map(|span| span.text.as_ref())
        .collect::<String>();
    assert!(oldest_row.starts_with("replacement-"));
    assert!(
        state
            .selected_text()?
            .is_none_or(|text| !text.contains("selected"))
    );
    Ok(())
}

#[test]
fn copy_is_observational_while_input_and_paste_clear_selection_and_keep_shell_encoding()
-> Result<()> {
    let mut state = terminal(32, 4)?;
    state.feed(b"copy me\x1b[?1h\x1b[?2004h");
    install(&mut state, cell(0, 0), cell(3, 0))?;
    for _ in 0..2 {
        assert_eq!(state.selected_text()?.as_deref(), Some("copy"));
    }
    assert!(state.has_selection());
    assert!(state.take_pty_writes().is_empty());
    assert_eq!(state.prepare_input(b"\x03")?, vec![3]);
    assert!(!state.has_selection());
    assert_eq!(state.prepare_input(b"\x1b[A")?, b"\x1bOA");
    install(&mut state, cell(0, 0), cell(3, 0))?;
    assert_eq!(
        state.prepare_paste("界\n")?,
        "\x1b[200~界\n\x1b[201~".as_bytes()
    );
    assert!(!state.has_selection());
    Ok(())
}

#[test]
fn edge_ticks_scroll_native_history_and_cancel_or_release_keep_completed_selection() -> Result<()> {
    let mut state = terminal(20, 4)?;
    for index in 0..30 {
        state.feed(format!("line-{index:02}\r\n"));
    }
    state.selection_press(cell(0, 1), 10.0, 19.0, 1)?;
    state.selection_drag(cell(0, 0), 10.0, -20.0, false, geometry(20, 4))?;
    assert_eq!(state.selection_autoscroll(), Autoscroll::Up);
    let before = state.terminal.scrollbar()?.offset;
    for _ in 0..3 {
        state.selection_tick(cell(0, 0), 10.0, -20.0, false, geometry(20, 4))?;
    }
    assert!(state.terminal.scrollbar()?.offset < before);
    let copied = state.selected_text()?;
    assert!(copied.is_some());
    state.selection_release(None)?;
    assert_eq!(state.selection_autoscroll(), Autoscroll::None);
    assert_eq!(state.selected_text()?, copied);
    state.cancel_selection_gesture();
    assert_eq!(state.selected_text()?, copied);
    assert!(state.selection.autoscroll.is_none());
    assert!(state.selection.held.is_none());
    Ok(())
}

#[test]
fn fractional_grid_geometry_preserves_late_column_thresholds_and_clamps_outside_points()
-> Result<()> {
    for (cell_width, cell_height) in [(7.8, 18.0), (12.25, 28.0), (15.6, 36.0)] {
        let grid = Grid::new(
            Bounds::new(point(px(50.0), px(90.0)), size(px(1800.0), px(400.0))),
            100,
            10,
            cell_width,
            cell_height,
        )
        .context("grid")?;
        let pointer = grid.origin + point(px(99.75 * cell_width), px(2.5 * cell_height));
        assert_eq!(grid.cell(pointer), cell(99, 2));
        let (x, _, geometry) = grid.surface(pointer);
        assert!((x - 9975.0).abs() < 0.02);
        assert_eq!(geometry.cell_width, 100);
        assert_eq!(
            grid.cell(grid.origin - point(px(100.0), px(100.0))),
            cell(0, 0)
        );
        assert_eq!(
            grid.cell(grid.origin + point(px(10_000.0), px(10_000.0))),
            cell(99, 9)
        );
    }
    assert!(
        Grid::new(
            Bounds::new(point(px(0.0), px(0.0)), size(px(20.0), px(10.0))),
            0,
            0,
            0.0,
            0.0
        )
        .is_none()
    );
    Ok(())
}
