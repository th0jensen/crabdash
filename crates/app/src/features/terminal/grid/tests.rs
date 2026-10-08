use super::{Glyph, compatible_ascii, glyph_intervals, glyph_x, overlapping_glyphs, x};
use crate::features::terminal::TerminalState;
use anyhow::Result;
use gpui::px;
use libghostty_vt::{
    selection::Selection,
    terminal::{Point, PointCoordinate},
};

fn terminal() -> Result<TerminalState> {
    TerminalState::new_interactive(32, 4, 1000, 8, 18)
}

fn metadata(state: &TerminalState) -> Vec<Vec<(u16, u16, String)>> {
    state
        .rendered
        .iter()
        .map(|row| {
            row.iter()
                .flat_map(|span| &span.clusters)
                .map(|cluster| (cluster.column, cluster.columns, cluster.text.to_string()))
                .collect()
        })
        .collect()
}

fn select(state: &mut TerminalState, first: u16, last: u16) -> Result<()> {
    {
        let start = state
            .terminal
            .grid_ref(Point::Viewport(PointCoordinate { x: first, y: 0 }))?;
        let end = state
            .terminal
            .grid_ref(Point::Viewport(PointCoordinate { x: last, y: 0 }))?;
        state
            .terminal
            .set_selection(Some(&Selection::new(start, end, false)))?;
    }
    state.refresh_frame();
    Ok(())
}

#[test]
fn native_cluster_columns_preserve_wide_combining_and_zwj_cells() -> Result<()> {
    let mut state = terminal()?;
    state.feed("A界e\u{301} 👩‍💻");
    let final_column = state.terminal.cursor_x()?;
    state.feed("Z");
    let cells = &metadata(&state)[0];
    assert_eq!(
        cells
            .iter()
            .map(|(_, _, text)| text.as_str())
            .collect::<String>(),
        "A界e\u{301} 👩‍💻Z"
    );
    assert_eq!(cells[0], (0, 1, "A".into()));
    assert_eq!(cells[1], (1, 2, "界".into()));
    assert_eq!(cells[2], (3, 1, "e\u{301}".into()));
    assert_eq!(cells.last(), Some(&(final_column, 1, "Z".into())));
    for pair in cells.windows(2) {
        assert_eq!(pair[0].0 + pair[0].1, pair[1].0);
    }
    assert!(
        cells
            .iter()
            .all(|(_, columns, _)| (1..=2).contains(columns))
    );
    Ok(())
}

#[test]
fn native_grapheme_mode_keeps_a_zwj_sequence_atomic() -> Result<()> {
    let mut state = terminal()?;
    state.feed("\x1b[?2027h👩‍💻");
    let end_column = state.terminal.cursor_x()?;
    state.feed("Z");
    let cells = &metadata(&state)[0];
    assert_eq!(cells[0], (0, 2, "👩‍💻".into()));
    assert_eq!(end_column, 2);
    assert_eq!(cells[1], (end_column, 1, "Z".into()));
    Ok(())
}

#[test]
fn selecting_final_unicode_cell_never_changes_native_glyph_origins() -> Result<()> {
    let mut state = terminal()?;
    state.feed("A界e\u{301} 👩‍💻");
    let column = state.terminal.cursor_x()?;
    state.feed("Z");
    let before = metadata(&state);
    select(&mut state, column, column)?;
    assert_eq!(metadata(&state), before);
    assert_eq!(state.selected_text()?.as_deref(), Some("Z"));
    let selected = state.rendered[0]
        .iter()
        .filter(|span| span.selected)
        .flat_map(|span| &span.clusters)
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].column, column);
    for width in [7.8, 12.0] {
        assert_eq!(
            x(selected[0].column, width),
            x(before[0].last().map(|cell| cell.0).unwrap_or(0), width)
        );
    }
    Ok(())
}

#[test]
fn blank_trimming_and_selected_blank_cells_keep_matching_cluster_extents() -> Result<()> {
    let mut state = terminal()?;
    state.feed("A   ");
    assert_eq!(metadata(&state)[0], [(0, 1, "A".into())]);
    select(&mut state, 2, 3)?;
    let selected = state.rendered[0]
        .iter()
        .filter(|span| span.selected)
        .flat_map(|span| &span.clusters)
        .collect::<Vec<_>>();
    assert_eq!(
        selected
            .iter()
            .map(|cell| (cell.column, cell.columns))
            .collect::<Vec<_>>(),
        [(2, 1), (3, 1)]
    );
    for span in &state.rendered[0] {
        assert_eq!(
            span.columns,
            span.clusters.iter().map(|cell| cell.columns).sum::<u16>()
        );
        assert_eq!(
            span.text.as_ref(),
            span.clusters
                .iter()
                .map(|cell| cell.text.as_ref())
                .collect::<String>()
        );
    }
    Ok(())
}

#[test]
fn style_changes_and_reflow_preserve_whole_native_cells() -> Result<()> {
    let mut state = terminal()?;
    state.feed("A\x1b[1;31m界e\u{301}\x1b[0mZ");
    assert!(
        state.rendered[0]
            .iter()
            .any(|span| span.bold && span.fg.is_some())
    );
    let text = metadata(&state)[0]
        .iter()
        .map(|cell| cell.2.as_str())
        .collect::<String>();
    assert_eq!(text, "A界e\u{301}Z");
    state.resize(4, 4, 8, 18)?;
    for row in metadata(&state) {
        assert!(row.iter().all(|(column, columns, _)| column + columns <= 4));
        assert!(row.iter().all(|(_, _, text)| text != "\u{301}"));
        assert!(
            row.windows(2)
                .all(|pair| pair[0].0 + pair[0].1 == pair[1].0)
        );
    }
    Ok(())
}

#[test]
fn cell_positions_are_fractional_logical_pixels_without_accumulated_rounding() {
    for width in [7.8, 12.0] {
        for scale in [1.0, 2.0] {
            assert_eq!(f32::from(x(99, width)) * scale, 99.0 * width * scale);
            assert_eq!(f32::from(x(2, width)), 2.0 * width);
            assert_eq!(
                glyph_x(99, px(99.0 * width + 0.01), true, width),
                x(99, width)
            );
            assert_eq!(glyph_x(99, px(1.2), false, width), px(1.2));
        }
    }
}

#[test]
fn ascii_batching_accepts_cell_aligned_ligatures_and_rejects_font_drift() {
    assert!(compatible_ascii(&[(0, 0.0), (2, 16.0)], 24.0, 3, 8.0));
    assert!(compatible_ascii(
        &[(0, 0.0), (1, 8.0), (2, 16.0)],
        24.0,
        3,
        8.0
    ));
    assert!(!compatible_ascii(
        &[(0, 0.0), (1, 7.2), (2, 16.0)],
        24.0,
        3,
        8.0
    ));
    assert!(!compatible_ascii(&[(0, 0.0), (1, 8.0)], 13.0, 2, 8.0));
    assert!(!compatible_ascii(&[(1, 8.0)], 16.0, 2, 8.0));
    assert!(!compatible_ascii(
        &[(0, 0.0), (2, 16.0), (1, 8.0)],
        24.0,
        3,
        8.0
    ));
    assert!(!compatible_ascii(&[(0, f32::NAN)], 8.0, 1, 8.0));
}

#[test]
fn alternating_foregrounds_visit_each_ascii_glyph_once() {
    let indices = (0..256).collect::<Vec<_>>();
    let glyphs = glyph_intervals(&indices, indices.len())
        .into_iter()
        .enumerate()
        .map(|(glyph, cells)| Glyph {
            run: 0,
            glyph,
            cells,
        })
        .collect::<Vec<_>>();
    let mut first = 0;
    let mut visits = 0;
    for column in 0..256 {
        let range = overlapping_glyphs(&glyphs, &mut first, column..column + 1);
        assert_eq!(range, column..column + 1);
        visits += range.len();
    }
    assert_eq!(visits, 256);
}

#[test]
fn a_crossing_ligature_and_repeated_glyph_indices_keep_their_native_origins() {
    let intervals = glyph_intervals(&[0, 0, 2], 3);
    assert_eq!(intervals, [0..2, 0..2, 2..3]);
    let glyphs = intervals
        .into_iter()
        .enumerate()
        .map(|(glyph, cells)| Glyph {
            run: 0,
            glyph,
            cells,
        })
        .collect::<Vec<_>>();
    let mut first = 0;
    assert_eq!(overlapping_glyphs(&glyphs, &mut first, 0..1), 0..2);
    assert_eq!(overlapping_glyphs(&glyphs, &mut first, 1..2), 0..2);
    assert_eq!(overlapping_glyphs(&glyphs, &mut first, 2..3), 2..3);
    assert_eq!(glyph_x(0, px(0.02), true, 7.8), px(0.0));
    assert_eq!(glyph_x(2, px(15.62), true, 7.8), px(15.6));
}
