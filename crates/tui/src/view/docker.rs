//! Containers with their state; actions apply to the selected row.
use super::{state_style, table};
use crate::{app::App, poll::short_id};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Cell, Row},
};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = app.docker.as_ref().map(|docker| {
        docker.as_deref().map_err(Clone::clone).map(|containers| {
            containers
                .iter()
                .map(|container| {
                    Row::new([
                        Cell::from(container.name.clone()),
                        Cell::from(container.status.clone()).style(state_style(&container.status)),
                        Cell::from(short_id(&container.id).to_owned()),
                    ])
                })
                .collect()
        })
    });
    table(
        frame,
        app,
        area,
        rows,
        Row::new(["Name", "State", "ID"]),
        &[
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Length(12),
        ],
        "No containers.",
    );
}
