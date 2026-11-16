//! Disks with their partitions and mounts, flattened into indented rows.
use super::{MUTED, state_color, table};
use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Style, Stylize},
    widgets::{Cell, Row},
};
use utils::disks::{Disk, DiskNode};

/// One row per disk and nested node: depth, name, size, and status or detail.
pub fn rows(disks: &[Disk]) -> Vec<(usize, &str, Option<&str>, Option<&str>)> {
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
    let loaded = app.disks.as_ref().map(|disks| {
        disks.as_deref().map_err(Clone::clone).map(|disks| {
            rows(disks)
                .into_iter()
                .map(|(depth, name, size, detail)| {
                    let detail = detail.unwrap_or_default().to_owned();
                    let (name, detail) = if depth == 0 {
                        (
                            Cell::from(name.to_owned()).bold(),
                            Cell::from(detail.clone()).style(Style::new().fg(state_color(&detail))),
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
            Constraint::Fill(1),
            Constraint::Length(9),
            Constraint::Fill(1),
        ],
        "No disks.",
    );
}
