//! Disks with their partitions and mounts, flattened into indented rows.
use super::{MUTED, name_column, state_style, table};
use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Stylize,
    widgets::{Cell, Row},
};
use utils::disks::{Disk, DiskNode};

/// One row per disk and nested node: depth, name, size, and status or detail.
pub fn rows<'a>(disks: &[&'a Disk]) -> Vec<(usize, &'a str, Option<&'a str>, Option<&'a str>)> {
    fn nodes<'a>(
        out: &mut Vec<(usize, &'a str, Option<&'a str>, Option<&'a str>)>,
        list: &'a [DiskNode],
        depth: usize,
    ) {
        for node in list {
            out.push((
                depth,
                &node.name,
                node.size.as_deref(),
                node.detail.as_deref(),
            ));
            nodes(out, &node.nodes, depth + 1);
        }
    }
    let mut out = Vec::new();
    for disk in disks {
        out.push((
            0,
            disk.name.as_str(),
            disk.size.as_deref(),
            Some(disk.status.as_str()),
        ));
        nodes(&mut out, &disk.nodes, 1);
    }
    out
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let disks = app.visible_disks();
    let rows_shown = rows(&disks);
    let longest = rows_shown
        .iter()
        .map(|(depth, name, ..)| name.chars().count() + depth * 2)
        .max();
    let loaded = app.disks.as_ref().map(|loaded| {
        loaded.as_ref().map_err(Clone::clone).map(|_| {
            rows_shown
                .into_iter()
                .map(|(depth, name, size, detail)| {
                    let detail = detail.unwrap_or_default().to_owned();
                    let (name, detail) = if depth == 0 {
                        (
                            Cell::from(name.to_owned()).bold(),
                            Cell::from(detail.clone()).style(state_style(&detail)),
                        )
                    } else {
                        (
                            Cell::from(format!("{}└ {name}", "  ".repeat(depth - 1))),
                            Cell::from(detail).style(MUTED),
                        )
                    };
                    Row::new([
                        name,
                        Cell::from(size.unwrap_or_default().to_owned()),
                        detail,
                    ])
                })
                .collect()
        })
    });
    table(
        frame,
        app,
        area,
        loaded,
        Row::new(["Disk", "Size", "Status"]),
        &[
            name_column(longest.unwrap_or_default(), area.width, 0.5),
            Constraint::Length(9),
            Constraint::Fill(1),
        ],
        "No disks.",
    );
}
