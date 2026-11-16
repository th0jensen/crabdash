//! Containers with their state; actions apply to the selected row.
use super::{MUTED, detail, name_column, state_style, table};
use crate::{app::App, poll::short_id};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::Span,
    widgets::{Cell, Row},
};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let containers = app.visible_containers();
    let longest = containers.iter().map(|c| c.name.chars().count()).max();
    let rows = app.docker.as_ref().map(|docker| {
        docker.as_ref().map_err(Clone::clone).map(|_| {
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
    // Columns sit beside the names; narrow panes drop the ID.
    let id = if area.width >= 40 { 12 } else { 0 };
    table(
        frame,
        app,
        area,
        rows,
        Row::new(["Name", "State", "ID"]),
        &[
            name_column(longest.unwrap_or_default(), area.width, 0.6),
            Constraint::Length(10),
            Constraint::Length(id),
        ],
        "No containers.",
    );
}

/// The expanded container's details: its state and ID at once, then what
/// Docker's inspection adds once it arrives.
pub fn details(app: &App) -> Option<Vec<Vec<Span<'static>>>> {
    let container = app.selected_container()?;
    let mut items = vec![
        detail("State", container.status.clone()),
        detail("ID", short_id(&container.id).to_owned()),
    ];
    match &app.details {
        Some((id, Ok(details))) if *id == container.id => {
            if let Some(image) = &details.image {
                items.push(detail("Image", image.clone()));
            }
            if let Some(health) = &details.health {
                items.push(detail("Health", health.clone()));
            }
            if let Some(policy) = &details.restart_policy {
                items.push(detail("Restart", policy.name.clone()));
            }
            let ports: Vec<String> = details
                .ports
                .iter()
                .flatten()
                .map(|port| {
                    let inner = format!("{}/{}", port.container_port, port.protocol);
                    match port.bindings.as_deref().and_then(<[_]>::first) {
                        Some(binding) => format!("{}→{inner}", binding.host_port),
                        None => inner,
                    }
                })
                .collect();
            if !ports.is_empty() {
                items.push(detail("Ports", ports.join(" ")));
            }
            let mounts: Vec<String> = details
                .mounts
                .iter()
                .flatten()
                .filter_map(|mount| mount.destination.clone())
                .collect();
            if !mounts.is_empty() {
                items.push(detail("Mounts", mounts.join(" ")));
            }
            if let Some(created) = &details.created {
                let day = created.get(..10).unwrap_or(created);
                items.push(detail("Created", day.to_owned()));
            }
        }
        Some((id, Err(error))) if *id == container.id => {
            items.push(vec![Span::styled(error.clone(), MUTED)]);
        }
        _ => items.push(vec![Span::styled("Loading details…", MUTED)]),
    }
    Some(items)
}
