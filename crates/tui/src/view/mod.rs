//! Frame layout: host header, section tabs, the section, key hints and logs.
mod disks;
mod docker;
mod services;
mod system;

pub use disks::rows as disk_rows;

use crate::{app::App, section::Section};
use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Row, Table, Wrap},
};

pub const ACCENT: Color = Color::Cyan;
pub const MUTED: Style = Style::new().fg(Color::DarkGray);
pub const SELECTED: Style = Style::new().add_modifier(Modifier::REVERSED);

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.hits = Default::default();
    let [header, body, status, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(u16::from(app.status.is_some())),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    draw_header(frame, app, header);
    let rule = Block::new().borders(Borders::TOP).border_style(MUTED);
    let section = rule.inner(body);
    frame.render_widget(rule, body);
    let body = section;
    if let Some(error) = &app.failed {
        message(
            frame,
            body,
            &format!("Could not read this machine: {error}"),
            true,
        );
    } else {
        match app.section {
            Section::Docker => docker::draw(frame, app, body),
            Section::Disks => disks::draw(frame, app, body),
            Section::Services => services::draw(frame, app, body),
            Section::System => system::draw(frame, app, body),
        }
    }
    draw_status(frame, app, status);
    draw_footer(frame, app, footer);
    draw_logs(frame, app, body);
}

fn draw_header(frame: &mut Frame, app: &mut App, area: Rect) {
    let mut spans = Vec::new();
    let mut x = area.x;
    for section in Section::ALL {
        let title = format!(" {} ", section.title());
        let width = title.chars().count() as u16;
        let style = if section == app.section {
            Style::new().fg(ACCENT).add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        if x + width <= area.right() {
            app.hits
                .tabs
                .push((Rect::new(x, area.y, width, 1), section));
        }
        spans.push(Span::styled(title, style));
        x += width;
    }
    let host = app
        .host
        .as_ref()
        .map_or_else(String::new, |(name, platform)| {
            format!("{name} · {platform} ")
        });
    let used = x.saturating_sub(area.x);
    let [tabs, right] =
        Layout::horizontal([Constraint::Length(used), Constraint::Fill(1)]).areas(area);
    frame.render_widget(Line::from(spans), tabs);
    // Hidden rather than cut when the pane is too narrow, as on a phone.
    if host.chars().count() < usize::from(right.width) {
        frame.render_widget(Line::from(host).style(MUTED).right_aligned(), right);
    }
}

/// Key hints for the shown section, naming what each does to the selected row.
/// Tab comes last, so narrow panes drop it first: the tabs above take taps.
fn hints(app: &App) -> Vec<(KeyCode, &'static str)> {
    if app.logs.is_some() {
        return vec![
            (KeyCode::Esc, "close"),
            (KeyCode::Up, "↑"),
            (KeyCode::Down, "↓"),
        ];
    }
    let mut keys = Vec::new();
    match app.section {
        Section::Docker => {
            let container = app.selected_container();
            let active = container.is_some_and(|c| c.is_active_status());
            keys.push((KeyCode::Char('s'), if active { "stop" } else { "start" }));
            keys.push((KeyCode::Char('r'), "restart"));
            if container.is_some_and(|c| c.is_paused()) {
                keys.push((KeyCode::Char('p'), "resume"));
            } else if container.is_some_and(|c| c.is_running_status()) {
                keys.push((KeyCode::Char('p'), "pause"));
            }
            keys.push((KeyCode::Char('l'), "logs"));
        }
        Section::Services => {
            let running = app.selected_service().is_some_and(|s| s.is_running());
            keys.push((KeyCode::Char('s'), if running { "stop" } else { "start" }));
            keys.push((KeyCode::Char('r'), "restart"));
            keys.push((KeyCode::Char('l'), "logs"));
            keys.push((
                KeyCode::Char('a'),
                if app.active_services { "all" } else { "active" },
            ));
        }
        Section::Disks | Section::System => {}
    }
    keys.push((KeyCode::Char('q'), "quit"));
    keys.push((KeyCode::Tab, "next"));
    keys
}

fn key_label(code: KeyCode) -> String {
    match code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Tab => "⇥".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Up => "k".into(),
        KeyCode::Down => "j".into(),
        _ => String::new(),
    }
}

fn draw_footer(frame: &mut Frame, app: &mut App, area: Rect) {
    let mut spans = Vec::new();
    let mut x = area.x;
    for (code, label) in hints(app) {
        let key = format!(" {} ", key_label(code));
        let text = format!("{label} ");
        let width = (key.chars().count() + text.chars().count()) as u16;
        if x + width > area.right() {
            break;
        }
        app.hits.keys.push((Rect::new(x, area.y, width, 1), code));
        spans.push(Span::styled(key, Style::new().fg(ACCENT).bold()));
        spans.push(Span::styled(text, MUTED));
        x += width;
    }
    frame.render_widget(Line::from(spans), area);
}

/// The last action's progress or outcome, on its own line so narrow panes show it.
fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let line = match &app.status {
        Some(Ok(text)) => Line::from(format!(" {text}")),
        Some(Err(error)) => Line::from(format!(" {}", error.lines().next().unwrap_or_default()))
            .style(Style::new().fg(Color::Red)),
        None => return,
    };
    frame.render_widget(line, area);
}

fn draw_logs(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(logs) = &app.logs else { return };
    let (text, style) = match &logs.text {
        Ok(text) if text.trim().is_empty() => ("No log lines.".to_owned(), MUTED),
        Ok(text) => (text.clone(), Style::new()),
        Err(error) => (error.clone(), Style::new().fg(Color::Red)),
    };
    let block = Block::bordered()
        .title(format!(" {} ", logs.title))
        .border_style(Style::new().fg(ACCENT));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text)
            .style(style)
            .wrap(Wrap { trim: false })
            .scroll((logs.scroll, 0))
            .block(block),
        area,
    );
}

/// A centered note for loading, empty and error states.
pub fn message(frame: &mut Frame, area: Rect, text: &str, error: bool) {
    let style = if error {
        Style::new().fg(Color::Red)
    } else {
        MUTED
    };
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(text)
            .style(style)
            .centered()
            .wrap(Wrap { trim: true }),
        middle,
    );
}

/// Draws a selectable table and records its rows for taps. `None` rows are
/// still loading; an error or an empty list shows a note instead.
pub fn table(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    loaded: Option<Result<Vec<Row<'static>>, String>>,
    header: Row<'static>,
    widths: &[Constraint],
    empty: &str,
) {
    let rows = match loaded {
        None => return message(frame, area, "Loading…", false),
        Some(Err(error)) => return message(frame, area, &error, true),
        Some(Ok(rows)) if rows.is_empty() => return message(frame, area, empty, false),
        Some(Ok(rows)) => rows,
    };
    let state = &mut app.tables[app.section.index()];
    frame.render_stateful_widget(
        Table::new(rows, widths)
            .header(header.style(MUTED))
            .column_spacing(2)
            .row_highlight_style(SELECTED),
        area,
        state,
    );
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    app.hits.rows = Some((body, state.offset()));
}

/// Green for healthy states, yellow for transitional ones, red for failures.
pub fn state_color(state: &str) -> Color {
    match state.to_ascii_lowercase().as_str() {
        "running" | "active" | "healthy" | "mounted" | "swap" => Color::Green,
        "paused" | "restarting" | "activating" | "deactivating" | "reloading" => Color::Yellow,
        "failed" | "dead" | "crashed" | "unhealthy" => Color::Red,
        _ => Color::DarkGray,
    }
}
