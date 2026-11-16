//! System services by name; actions apply to the selected row.
use super::{MUTED, state_color, table};
use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Cell, Row},
};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let loaded = app.services.as_ref().map(|services| {
        services.as_ref().map_err(Clone::clone).map(|_| {
            app.visible_services()
                .into_iter()
                .map(|service| {
                    let state = service.status_label();
                    Row::new([
                        Cell::from(service.name.trim_end_matches(".service").to_owned()),
                        Cell::from(state).style(Style::new().fg(state_color(state))),
                        Cell::from(service.description.clone().unwrap_or_default()).style(MUTED),
                    ])
                })
                .collect()
        })
    });
    let empty = if app.active_services {
        "No active services. Press a to show all."
    } else {
        "No services."
    };
    table(
        frame,
        app,
        area,
        loaded,
        Row::new(["Service", "State", "Description"]),
        &[
            Constraint::Fill(2),
            Constraint::Length(9),
            // Phone-width panes give names the room instead.
            if area.width >= 60 {
                Constraint::Fill(3)
            } else {
                Constraint::Length(0)
            },
        ],
        empty,
    );
}
