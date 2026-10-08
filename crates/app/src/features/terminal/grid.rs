//! Native cell geometry, independent of fallback glyph advances and selection colors.
use super::{TerminalCluster, TerminalCursor, TerminalSpan, selection};
use crate::features::preferences::Preferences;
use gpui::{prelude::*, *};
use std::ops::Range;

struct Cell {
    cluster: TerminalCluster,
    foreground: Rgba,
    background: Option<Rgba>,
    bold: bool,
}

struct Batch {
    line: ShapedLine,
    column: u16,
    cells: Vec<Cell>,
    ascii_grid: bool,
    glyphs: Vec<Glyph>,
}

struct Glyph {
    run: usize,
    glyph: usize,
    cells: Range<usize>,
}

impl Batch {
    fn new(line: ShapedLine, column: u16, cells: Vec<Cell>, ascii_grid: bool) -> Self {
        let indices = line
            .runs
            .iter()
            .flat_map(|run| &run.glyphs)
            .map(|glyph| glyph.index)
            .collect::<Vec<_>>();
        let intervals = if ascii_grid {
            glyph_intervals(&indices, cells.len())
        } else {
            vec![0..usize::from(cells[0].cluster.columns); indices.len()]
        };
        let glyphs = line
            .runs
            .iter()
            .enumerate()
            .flat_map(|(run, shaped)| (0..shaped.glyphs.len()).map(move |glyph| (run, glyph)))
            .zip(intervals)
            .map(|((run, glyph), cells)| Glyph { run, glyph, cells })
            .collect();
        Self {
            line,
            column,
            cells,
            ascii_grid,
            glyphs,
        }
    }
}

/// Repeated indices share one cluster interval; a ligature may cover several cells.
fn glyph_intervals(indices: &[usize], len: usize) -> Vec<Range<usize>> {
    let mut end = len;
    let mut previous = len;
    let mut intervals = Vec::with_capacity(indices.len());
    for &index in indices.iter().rev() {
        if index != previous {
            end = previous;
        }
        intervals.push(index..end);
        previous = index;
    }
    intervals.reverse();
    intervals
}

/// Foreground masks arrive in cell order. Advance once past each completed glyph.
fn overlapping_glyphs(glyphs: &[Glyph], first: &mut usize, cells: Range<usize>) -> Range<usize> {
    while *first < glyphs.len() && glyphs[*first].cells.end <= cells.start {
        *first += 1;
    }
    let mut end = *first;
    while end < glyphs.len() && glyphs[end].cells.start < cells.end {
        end += 1;
    }
    *first..end
}

fn x(column: u16, cell_width: f32) -> Pixels {
    px(f32::from(column) * cell_width)
}

/// Only batch ASCII when the shaped anchors and advances already obey the cell grid.
/// This retains compatible ligatures while rejecting kerning/proportional-font drift.
fn compatible_ascii(anchors: &[(usize, f32)], width: f32, len: usize, cell_width: f32) -> bool {
    let tolerance = 0.05;
    width.is_finite()
        && (width - len as f32 * cell_width).abs() <= tolerance
        && !anchors.is_empty()
        && anchors.first().is_some_and(|(index, _)| *index == 0)
        && anchors.windows(2).all(|pair| pair[0].0 <= pair[1].0)
        && anchors.iter().all(|(index, position)| {
            *index < len
                && position.is_finite()
                && (*position - *index as f32 * cell_width).abs() <= tolerance
        })
}

fn shape(text: SharedString, bold: bool, settings: &Preferences, window: &Window) -> ShapedLine {
    let mut terminal_font = font(settings.terminal_font.clone());
    if bold {
        terminal_font.weight = FontWeight::BOLD;
    }
    let run = TextRun {
        len: text.len(),
        font: terminal_font,
        color: white(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    // Neutral color keeps geometry independent of foreground/selection segmentation.
    window
        .text_system()
        .shape_line(text, px(settings.terminal_font_size), &[run], None)
}

fn batches(
    cells: Vec<Cell>,
    settings: &Preferences,
    cell_width: f32,
    window: &Window,
) -> Vec<Batch> {
    let mut cells = cells.into_iter().peekable();
    let mut result = Vec::new();
    while let Some(first) = cells.next() {
        let column = first.cluster.column;
        let bold = first.bold;
        let ascii = first.cluster.columns == 1
            && first.cluster.text.len() == 1
            && first.cluster.text.is_ascii();
        let mut group = vec![first];
        if ascii {
            while cells.peek().is_some_and(|next| {
                next.bold == bold
                    && next.cluster.columns == 1
                    && next.cluster.text.len() == 1
                    && next.cluster.text.is_ascii()
                    && usize::from(next.cluster.column) == usize::from(column) + group.len()
            }) {
                if let Some(next) = cells.next() {
                    group.push(next);
                }
            }
        }
        let text = group
            .iter()
            .map(|cell| cell.cluster.text.as_ref())
            .collect::<String>();
        let line = shape(text.into(), bold, settings, window);
        let anchors = line
            .runs
            .iter()
            .flat_map(|run| &run.glyphs)
            .map(|glyph| (glyph.index, f32::from(glyph.position.x)))
            .collect::<Vec<_>>();
        let ascii_grid =
            ascii && compatible_ascii(&anchors, f32::from(line.width), group.len(), cell_width);
        if group.len() == 1 || ascii_grid {
            result.push(Batch::new(line, column, group, ascii_grid));
        } else {
            // Shape whole native clusters independently; never divide UTF-8/grapheme data.
            for cell in group {
                let line = shape(cell.cluster.text.clone(), cell.bold, settings, window);
                result.push(Batch::new(line, cell.cluster.column, vec![cell], false));
            }
        }
    }
    result
}

fn paint_glyphs(
    batch: &Batch,
    origin: Point<Pixels>,
    baseline: Pixels,
    window: &mut Window,
    color: Rgba,
    cell_width: f32,
    glyphs: Range<usize>,
) {
    for entry in &batch.glyphs[glyphs] {
        let run = &batch.line.runs[entry.run];
        let glyph = &run.glyphs[entry.glyph];
        let position = point(
            origin.x + glyph_x(glyph.index, glyph.position.x, batch.ascii_grid, cell_width),
            // Match GPUI's painter; platform glyph y coordinates have different axes.
            origin.y + baseline,
        );
        let result = if glyph.is_emoji {
            window.paint_emoji(position, run.font_id, glyph.id, batch.line.font_size)
        } else {
            window.paint_glyph(
                position,
                run.font_id,
                glyph.id,
                batch.line.font_size,
                color.into(),
            )
        };
        if let Err(error) = result {
            tracing::debug!(%error, "Failed to paint terminal glyph");
        }
    }
}

fn glyph_x(index: usize, shaped: Pixels, ascii_grid: bool, cell_width: f32) -> Pixels {
    if ascii_grid {
        px(index as f32 * cell_width)
    } else {
        shaped
    }
}

pub(super) fn render(
    logs: &[Vec<TerminalSpan>],
    cursor: Option<TerminalCursor>,
    default_fg: Rgba,
    settings: Preferences,
    cell_width: f32,
    line_height: f32,
    cx: &App,
) -> Div {
    let rows = logs
        .iter()
        .map(|row| {
            row.iter()
                .flat_map(|span| {
                    let (foreground, background) = if span.selected {
                        selection::colors(span.bg, &settings, cx)
                    } else {
                        (span.fg.unwrap_or(default_fg), span.bg)
                    };
                    span.clusters.iter().cloned().map(move |cluster| Cell {
                        cluster,
                        foreground,
                        background,
                        bold: span.bold,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let columns = rows
        .iter()
        .flat_map(|row| row.iter())
        .map(|cell| cell.cluster.column.saturating_add(cell.cluster.columns))
        .max()
        .unwrap_or(0)
        .max(
            cursor
                .map(|cursor| cursor.column.saturating_add(1))
                .unwrap_or(0),
        );
    let height = rows.len() as f32 * line_height;
    let width = f32::from(columns) * cell_width;
    div()
        .w(px(width))
        .min_w(px(width))
        .h(px(height))
        .flex_none()
        .child(
            canvas(
                move |_, window, _| {
                    // Use the primary shaped line's positive ascent/descent, as the
                    // stock painter does. Platform FontMetrics use different signs.
                    let primary = shape("M".into(), false, &settings, window);
                    let baseline =
                        (px(line_height) - primary.ascent - primary.descent) / 2.0 + primary.ascent;
                    let rows = rows
                        .into_iter()
                        .map(|row| batches(row, &settings, cell_width, window))
                        .collect::<Vec<_>>();
                    (rows, baseline)
                },
                move |bounds, (rows, baseline), window, _| {
                    window.with_content_mask(Some(ContentMask { bounds }), |window| {
                        for (row_index, batches) in rows.iter().enumerate() {
                            let row_origin =
                                bounds.origin + point(px(0.0), px(row_index as f32 * line_height));
                            for batch in batches {
                                for cell in &batch.cells {
                                    if let Some(background) = cell.background {
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                row_origin
                                                    + point(
                                                        x(cell.cluster.column, cell_width),
                                                        px(0.0),
                                                    ),
                                                size(
                                                    x(cell.cluster.columns, cell_width),
                                                    px(line_height),
                                                ),
                                            ),
                                            background,
                                        ));
                                    }
                                }
                            }
                            for batch in batches {
                                let origin =
                                    row_origin + point(x(batch.column, cell_width), px(0.0));
                                let mut start = 0;
                                let mut first_glyph = 0;
                                while start < batch.cells.len() {
                                    let color = batch.cells[start].foreground;
                                    let mut end = start + 1;
                                    while end < batch.cells.len()
                                        && batch.cells[end].foreground == color
                                    {
                                        end += 1;
                                    }
                                    if start == 0 && end == batch.cells.len() {
                                        paint_glyphs(
                                            batch,
                                            origin,
                                            baseline,
                                            window,
                                            color,
                                            cell_width,
                                            0..batch.glyphs.len(),
                                        );
                                    } else {
                                        let first = &batch.cells[start].cluster;
                                        let last = &batch.cells[end - 1].cluster;
                                        let glyphs = overlapping_glyphs(
                                            &batch.glyphs,
                                            &mut first_glyph,
                                            usize::from(first.column - batch.column)
                                                ..usize::from(
                                                    last.column + last.columns - batch.column,
                                                ),
                                        );
                                        let mask = Bounds::new(
                                            row_origin
                                                + point(x(first.column, cell_width), px(0.0)),
                                            size(
                                                x(
                                                    last.column + last.columns - first.column,
                                                    cell_width,
                                                ),
                                                px(line_height),
                                            ),
                                        );
                                        window.with_content_mask(
                                            Some(ContentMask { bounds: mask }),
                                            |window| {
                                                paint_glyphs(
                                                    batch, origin, baseline, window, color,
                                                    cell_width, glyphs,
                                                );
                                            },
                                        );
                                    }
                                    start = end;
                                }
                            }
                        }
                        if let Some(cursor) = cursor {
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin
                                        + point(
                                            x(cursor.column, cell_width),
                                            px(f32::from(cursor.row) * line_height + 2.0),
                                        ),
                                    size(px(1.5), px((line_height - 4.0).max(0.0))),
                                ),
                                rgb(0xD7D7D7),
                            ));
                        }
                    });
                },
            )
            .size_full(),
        )
}

#[cfg(test)]
mod tests;
