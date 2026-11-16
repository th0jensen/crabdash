//! Frame layout in ramp's visual language: an inset rounded frame titled with
//! the host, section toggles, the section, key hints and logs. Colors are the
//! terminal's own ANSI roles, so Rex themes apply.
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
    widgets::{Block, BorderType, Clear, HighlightSpacing, Padding, Paragraph, Row, Table, Wrap},
};

/// Selection and focus, ramp's accent.
pub const ACCENT: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
/// Charts, ramp's highlight role.
pub const HIGHLIGHT: Color = Color::Cyan;
/// Secondary text: the foreground, dimmed.
pub const MUTED: Style = Style::new().add_modifier(Modifier::DIM);
const BORDER: Style = Style::new().fg(Color::DarkGray);
const ERROR: Style = Style::new().fg(Color::Red);

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.hits = Default::default();
    let [panel, status, footer] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(status_height(app, frame.area().width)),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let body = draw_panel(frame, app, panel);
    let [toggles, body] =
        Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(body);
    draw_toggles(frame, app, toggles);
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
    draw_footer(frame, app, inset(footer));
    draw_logs(frame, app, panel);
}

/// One blank column on each side, like ramp's composer and panels.
fn inset(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(2),
        ..area
    }
}

/// A rounded frame with its labels in the border, dimmed as in ramp.
fn frame_block(top: String, bottom: Option<String>) -> Block<'static> {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(BORDER)
        .padding(Padding::horizontal(1))
        .title(Line::styled(top, MUTED));
    match bottom {
        Some(bottom) => block.title_bottom(Line::styled(bottom, MUTED).right_aligned()),
        None => block,
    }
}

/// The section's frame, titled with the machine and summarizing the section.
fn draw_panel(frame: &mut Frame, app: &App, area: Rect) -> Rect {
    let area = inset(area);
    let host = app.host.as_ref().map_or_else(
        || " crabdash ".into(),
        |(name, platform)| format!(" {name} · {platform} "),
    );
    let block = frame_block(host, summary(app).map(|text| format!(" {text} ")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn count(items: usize, noun: &str) -> String {
    let plural = match (items, noun.ends_with('s')) {
        (1, _) => "",
        (_, true) => "es",
        (_, false) => "s",
    };
    format!("{items} {noun}{plural}")
}

/// What the bottom border says about the shown section.
fn summary(app: &App) -> Option<String> {
    match app.section {
        Section::Docker => {
            let containers = app.docker.as_ref()?.as_ref().ok()?;
            let running = containers.iter().filter(|c| c.is_running_status()).count();
            Some(format!(
                "{} · {running} running",
                count(containers.len(), "container")
            ))
        }
        Section::Disks => Some(count(app.disks.as_ref()?.as_ref().ok()?.len(), "disk")),
        Section::Services => {
            let services = app.services.as_ref()?.as_ref().ok()?;
            let active = services.iter().filter(|s| s.is_running()).count();
            Some(format!("{active} active · {} total", services.len()))
        }
        Section::System => {
            let usage = app.usage.as_ref()?.as_ref().ok()?;
            let uptime = format!("up {}", system::uptime(usage.uptime_seconds));
            Some(match usage.process_count {
                Some(processes) => format!("{} · {uptime}", count(processes, "process")),
                None => uptime,
            })
        }
    }
}

/// Section toggles, two columns apart: the shown one in the accent, the
/// others dimmed, like ramp's change scopes.
fn draw_toggles(frame: &mut Frame, app: &mut App, area: Rect) {
    let mut spans = Vec::new();
    let mut x = area.x;
    for section in Section::ALL {
        let title = section.title();
        let width = title.chars().count() as u16;
        if x + width > area.right() {
            break;
        }
        app.hits
            .tabs
            .push((Rect::new(x, area.y, width, 1), section));
        let style = if section == app.section {
            ACCENT
        } else {
            MUTED
        };
        spans.push(Span::styled(title, style));
        spans.push(Span::raw("  "));
        x += width + 2;
    }
    frame.render_widget(Line::from(spans), area);
}

/// Key hints for the shown section, naming what each does to the selected row.
/// Tab comes last, so narrow panes drop it first: the toggles take taps.
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
    let mut x = area.x + 1;
    spans.push(Span::raw(" "));
    for (code, label) in hints(app) {
        let key = key_label(code);
        let width = (key.chars().count() + 1 + label.chars().count()) as u16;
        if x + width > area.right() {
            break;
        }
        app.hits.keys.push((Rect::new(x, area.y, width, 1), code));
        spans.push(Span::styled(key, ACCENT));
        spans.push(Span::styled(format!(" {label}"), MUTED));
        spans.push(Span::raw("  "));
        x += width + 2;
    }
    frame.render_widget(Line::from(spans), area);
}

fn status_text(app: &App) -> Option<(String, Style)> {
    match &app.status {
        Some(Ok(text)) => Some((text.clone(), MUTED)),
        Some(Err(error)) => Some((error.lines().next().unwrap_or_default().to_owned(), ERROR)),
        None => None,
    }
}

/// Up to three wrapped lines, so a narrow pane still shows why an action failed.
fn status_height(app: &App, width: u16) -> u16 {
    let Some((text, _)) = status_text(app) else {
        return 0;
    };
    let width = usize::from(width.saturating_sub(4).max(1));
    text.chars().count().div_ceil(width).clamp(1, 3) as u16
}

/// The last action's progress or outcome, on its own lines below the frame,
/// lined up with the key hints.
fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let Some((text, style)) = status_text(app) else {
        return;
    };
    let area = inset(inset(area));
    frame.render_widget(
        Paragraph::new(text).style(style).wrap(Wrap { trim: true }),
        area,
    );
}

/// Logs float over the section in a frame of their own, titled in the accent
/// because it has focus.
fn draw_logs(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(logs) = &app.logs else { return };
    let (text, style) = match &logs.text {
        Ok(text) if text.trim().is_empty() => ("No log lines.".to_owned(), MUTED),
        Ok(text) => (text.clone(), Style::new()),
        Err(error) => (error.clone(), ERROR),
    };
    let area = inset(area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(BORDER)
        .padding(Padding::horizontal(1))
        .title(Line::styled(format!(" {} ", logs.title), ACCENT));
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
    let style = if error { ERROR } else { MUTED };
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
/// still loading; an error or an empty list shows a note instead. The selected
/// row is marked and drawn in the accent, as in ramp's lists.
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
    let selectable = app.row_count() > 0;
    let state = &mut app.tables[app.section.index()];
    frame.render_stateful_widget(
        Table::new(rows, widths)
            .header(header.style(MUTED))
            .column_spacing(2)
            .row_highlight_style(ACCENT)
            .highlight_symbol("› ")
            .highlight_spacing(if selectable {
                HighlightSpacing::Always
            } else {
                HighlightSpacing::Never
            }),
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

/// Success for healthy states, warning for transitional ones, error for
/// failures, and dimmed for everything at rest.
pub fn state_style(state: &str) -> Style {
    match state.to_ascii_lowercase().as_str() {
        "running" | "active" | "healthy" | "mounted" | "swap" => Style::new().fg(Color::Green),
        "paused" | "restarting" | "activating" | "deactivating" | "reloading" => {
            Style::new().fg(Color::Yellow)
        }
        "failed" | "dead" | "crashed" | "unhealthy" => ERROR,
        _ => MUTED,
    }
}
