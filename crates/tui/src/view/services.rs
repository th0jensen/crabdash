//! System services by name; actions apply to the selected row.
use super::{MUTED, detail, name_column, state_style, table};
use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Cell, Row},
};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let services = app.visible_services();
    let name = |s: &str| s.trim_end_matches(".service").to_owned();
    let longest = services.iter().map(|s| name(&s.name).chars().count()).max();
    let loaded = app.services.as_ref().map(|loaded| {
        loaded.as_ref().map_err(Clone::clone).map(|_| {
            services
                .iter()
                .map(|service| {
                    let state = service.status_label();
                    Row::new([
                        Cell::from(name(&service.name)),
                        Cell::from(state).style(state_style(state)),
                        Cell::from(service.description.clone().unwrap_or_default()).style(MUTED),
                    ])
                })
                .collect()
        })
    });
    // Phone-width panes give names the room; wider ones add descriptions.
    let described = area.width >= 50;
    table(
        frame,
        app,
        area,
        loaded,
        Row::new(["Service", "State", "Description"]),
        &[
            if described {
                name_column(longest.unwrap_or_default(), area.width, 0.4)
            } else {
                Constraint::Fill(1)
            },
            Constraint::Length(8),
            if described {
                Constraint::Fill(1)
            } else {
                Constraint::Length(0)
            },
        ],
        "No services.",
    );
}

/// The expanded service's state, unit file and description.
pub fn details(app: &App) -> Option<Vec<Vec<Span<'static>>>> {
    let service = app.selected_service()?;
    let mut state = service.status_label().to_owned();
    if let Some(sub) = service.sub_state.as_deref().filter(|s| !s.is_empty()) {
        state = format!("{state} ({sub})");
    }
    let mut items = vec![detail("State", state)];
    if let Some(file) = service.unit_file_state.as_deref().filter(|s| !s.is_empty()) {
        items.push(detail("Unit", file.to_owned()));
    }
    if let Some(load) = service.load_state.as_deref().filter(|s| !s.is_empty()) {
        items.push(detail("Load", load.to_owned()));
    }
    if let Some(description) = &service.description {
        items.push(vec![Span::styled(description.clone(), MUTED)]);
    }
    if let Some(error) = &service.error {
        items.push(vec![Span::styled(error.clone(), super::ERROR)]);
    }
    Some(items)
}
