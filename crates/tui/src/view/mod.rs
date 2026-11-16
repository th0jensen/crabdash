//! Frame layout in ramp's visual language: an inset rounded frame titled with
//! the host, section toggles, the section, its action sheet and logs. Colors
//! are the terminal's own ANSI roles, so Rex themes apply. The frame takes the
//! whole pane: keys go unlisted, and a tapped row opens its actions in a sheet
//! of buttons, stacked for a finger below ramp's desktop width.
mod disks;
mod docker;
mod services;
mod system;

pub use disks::rows as disk_rows;

use crate::{app::App, poll::short_id, section::Section, touch::DESKTOP_WIDTH};
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

/// A button: the key it presses and what it does.
type Button = (KeyCode, &'static str);

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.hits = Default::default();
    let phone = frame.area().width < DESKTOP_WIDTH;
    let [panel, status] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(status_height(app, frame.area().width)),
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
        // The sheet docks below the table, so the selected row stays in view.
        let sheet = sheet(app).map(|sheet| {
            let layout = SheetLayout::new(&sheet, body, phone);
            (sheet, layout)
        });
        let [body, sheet_area] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(sheet.as_ref().map_or(0, |(_, layout)| layout.height)),
        ])
        .areas(body);
        match app.section {
            Section::Docker => docker::draw(frame, app, body),
            Section::Disks => disks::draw(frame, app, body),
            Section::Services => services::draw(frame, app, body),
            Section::System => system::draw(frame, app, body),
        }
        if let Some((sheet, layout)) = sheet {
            draw_sheet(frame, app, sheet_area, &sheet, &layout);
        }
    }
    draw_status(frame, app, status);
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

fn rounded() -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(BORDER)
        .padding(Padding::horizontal(1))
}

/// A `×` in a frame's top right corner that closes it, as ramp's modals have.
/// Its target reaches one row into the frame, so a fingertip finds it.
fn close_button(app: &mut App, area: Rect) -> Line<'static> {
    let target = Rect::new(area.right().saturating_sub(6), area.y, 6, 2).intersection(area);
    app.hits.keys.push((target, KeyCode::Esc));
    Line::styled(" × ", ACCENT).right_aligned()
}

/// The section's frame, titled with the machine and summarizing the section.
/// Services' count is a toggle for active only, in the accent while it filters.
fn draw_panel(frame: &mut Frame, app: &mut App, area: Rect) -> Rect {
    let area = inset(area);
    let host = app.host.as_ref().map_or_else(
        || " crabdash ".into(),
        |(name, platform)| format!(" {name} · {platform} "),
    );
    let mut block = rounded().title(Line::styled(host, MUTED));
    if let Some(summary) = summary(app).map(|text| format!(" {text} ")) {
        let mut style = MUTED;
        if app.section == Section::Services {
            let width = summary.chars().count() as u16;
            let x = area.right().saturating_sub(width + 1);
            let target = Rect::new(x, area.bottom().saturating_sub(1), width, 1);
            app.hits.keys.push((target, KeyCode::Char('a')));
            if app.active_services {
                style = ACCENT;
            }
        }
        block = block.title_bottom(Line::styled(summary, style).right_aligned());
    }
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
/// others dimmed, like ramp's change scopes. Each target takes the gap beside
/// it and the blank row below, so taps between toggles still land.
fn draw_toggles(frame: &mut Frame, app: &mut App, area: Rect) {
    let mut spans = Vec::new();
    let mut x = area.x;
    for section in Section::ALL {
        let title = section.title();
        let width = title.chars().count() as u16;
        if x + width > area.right() {
            break;
        }
        let target = Rect::new(x.saturating_sub(1), area.y, width + 2, area.height);
        app.hits.tabs.push((target, section));
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

/// What the selected container or service can do, as buttons.
fn actions(app: &App) -> Vec<Button> {
    let mut buttons = Vec::new();
    match app.section {
        Section::Docker => {
            let Some(container) = app.selected_container() else {
                return buttons;
            };
            let active = container.is_active_status();
            buttons.push((KeyCode::Char('s'), if active { "stop" } else { "start" }));
            buttons.push((KeyCode::Char('r'), "restart"));
            if container.is_paused() {
                buttons.push((KeyCode::Char('p'), "resume"));
            } else if container.is_running_status() {
                buttons.push((KeyCode::Char('p'), "pause"));
            }
        }
        Section::Services => {
            let Some(service) = app.selected_service() else {
                return buttons;
            };
            let running = service.is_running();
            buttons.push((KeyCode::Char('s'), if running { "stop" } else { "start" }));
            buttons.push((KeyCode::Char('r'), "restart"));
        }
        Section::Disks | Section::System => return buttons,
    }
    buttons.push((KeyCode::Char('l'), "logs"));
    buttons
}

/// A pill: the label on the terminal's surface color, padded a cell on each
/// side as ramp pads its suggestions.
fn pill(app: &App, label: &str) -> Line<'static> {
    Line::raw(format!(" {label} ")).style(app.palette.pill)
}

/// Draws a pill and records it, so a tap on it presses its key.
fn draw_pill(frame: &mut Frame, app: &mut App, button: Button, line: Line<'static>, area: Rect) {
    app.hits.keys.push((area, button.0));
    frame.render_widget(line, area);
}

/// The selected row's actions, while their sheet is open.
struct Sheet {
    title: String,
    detail: String,
    buttons: Vec<Button>,
}

fn sheet(app: &App) -> Option<Sheet> {
    if !app.sheet || app.logs.is_some() {
        return None;
    }
    let (title, detail) = match app.section {
        Section::Docker => {
            let container = app.selected_container()?;
            let id = short_id(&container.id);
            (
                container.name.clone(),
                format!("{} · {id}", container.status),
            )
        }
        Section::Services => {
            let service = app.selected_service()?;
            let mut detail = service.status_label().to_owned();
            if let Some(description) = &service.description {
                detail = format!("{detail} · {description}");
            }
            let name = service.name.trim_end_matches(".service").to_owned();
            (name, detail)
        }
        Section::Disks | Section::System => return None,
    };
    Some(Sheet {
        title,
        detail,
        buttons: actions(app),
    })
}

/// Where a sheet's buttons go: stacked full width, one per row with a blank
/// row between, like a phone's action sheet, when the pane is narrow and tall
/// enough; otherwise in a wrapping row of pills.
struct SheetLayout {
    height: u16,
    stacked: bool,
    /// Each button's column offset, row and width inside the sheet.
    places: Vec<(u16, u16, u16)>,
}

impl SheetLayout {
    /// Border, detail line and the blank row below it.
    const CHROME: u16 = 4;
    /// Table rows the sheet leaves above itself.
    const TABLE: u16 = 5;

    fn new(sheet: &Sheet, body: Rect, phone: bool) -> Self {
        let inner = body.width.saturating_sub(4).max(1);
        let stacked_height = Self::CHROME + (sheet.buttons.len() as u16 * 2).saturating_sub(1);
        if phone && stacked_height + Self::TABLE <= body.height {
            let places = (0..sheet.buttons.len() as u16)
                .map(|row| (0, row * 2, inner))
                .collect();
            return Self {
                height: stacked_height,
                stacked: true,
                places,
            };
        }
        let (mut x, mut row) = (0, 0);
        let mut places = Vec::new();
        for (_, label) in &sheet.buttons {
            let width = label.chars().count() as u16 + 2;
            if x > 0 && x + width > inner {
                (x, row) = (0, row + 2);
            }
            places.push((x, row, width.min(inner)));
            x += width + 1;
        }
        Self {
            height: Self::CHROME + row + 1,
            stacked: false,
            places,
        }
    }
}

/// The selected row's actions in a rounded sheet titled with its name, closed
/// by its `×`, by Esc or by a tap outside it.
fn draw_sheet(frame: &mut Frame, app: &mut App, area: Rect, sheet: &Sheet, layout: &SheetLayout) {
    let close = close_button(app, area);
    let block = rounded()
        .title(Line::styled(format!(" {} ", sheet.title), ACCENT))
        .title(close);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Line::styled(sheet.detail.clone(), MUTED),
        Rect { height: 1, ..inner },
    );
    for (&button, &(x, row, width)) in sheet.buttons.iter().zip(&layout.places) {
        let place = Rect::new(inner.x + x, inner.y + 2 + row, width, 1).intersection(inner);
        let line = pill(app, button.1);
        if layout.stacked {
            // Each target takes the blank row above its button, so the
            // stack has no dead rows between targets.
            app.hits.keys.push((
                Rect {
                    y: place.y - 1,
                    height: 2,
                    ..place
                },
                button.0,
            ));
            frame.render_widget(line.centered(), place);
        } else {
            draw_pill(frame, app, button, line, place);
        }
    }
    app.hits.overlay = Some(area);
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

/// The last action's progress or outcome, on its own lines below the frame.
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
/// because it has focus. They follow the newest line until scrolled back.
fn draw_logs(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(logs) = &app.logs else { return };
    let (text, style) = match &logs.text {
        Ok(text) if text.trim().is_empty() => ("No log lines.".to_owned(), MUTED),
        Ok(text) => (text.clone(), Style::new()),
        Err(error) => (error.clone(), ERROR),
    };
    let title = Line::styled(format!(" {} ", logs.title), ACCENT);
    let area = inset(area);
    let close = close_button(app, area);
    let block = rounded().title(title).title(close);
    let inner = block.inner(area);
    let paragraph = Paragraph::new(text).style(style).wrap(Wrap { trim: false });
    let lines = paragraph.line_count(inner.width);
    let Some(logs) = &mut app.logs else { return };
    logs.max = lines
        .saturating_sub(usize::from(inner.height))
        .min(usize::from(u16::MAX)) as u16;
    logs.offset = if logs.follow {
        logs.max
    } else {
        logs.offset.min(logs.max)
    };
    frame.render_widget(Clear, area);
    frame.render_widget(paragraph.scroll((logs.offset, 0)).block(block), area);
    app.hits.overlay = Some(area);
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
    let selectable = app.selectable();
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
