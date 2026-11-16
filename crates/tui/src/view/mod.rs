//! Layout for Rex panes, in ramp's visual language without frames: the top
//! row holds the section tabs and a summary, the next a search field with the
//! section's filter chips, and the section fills the rest. Colors are the
//! terminal's own ANSI roles and reported surface, so Rex themes apply. Every
//! control takes taps and keys; a tapped row, or Tab, expands it in place with
//! its details and action buttons, beside each other in wide panes.
mod disks;
mod docker;
mod services;
mod system;

pub use disks::rows as disk_rows;

use crate::{app::App, search, section::Section};
use ratatui::{
    Frame,
    crossterm::event::KeyCode,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, HighlightSpacing, Paragraph, Row, Table, Wrap},
};

/// Selection and focus, ramp's accent.
pub const ACCENT: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
/// Charts, ramp's highlight role.
pub const HIGHLIGHT: Color = Color::Cyan;
/// Secondary text: the foreground, dimmed.
pub const MUTED: Style = Style::new().add_modifier(Modifier::DIM);
const ERROR: Style = Style::new().fg(Color::Red);

/// A button: the key it presses and what it does.
type Button = (KeyCode, &'static str);

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.hits = Default::default();
    let area = inset(frame.area());
    // A blank row under the search field when there is room to breathe.
    let gap = u16::from(area.height >= 16);
    let [header, field, _, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(gap),
        Constraint::Fill(1),
        Constraint::Length(status_height(app, area.width)),
    ])
    .areas(area);
    draw_header(frame, app, header);
    draw_search(frame, app, field);
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
    draw_logs(frame, app, body);
}

/// One blank column on each side, so text clears Rex's pane edges.
fn inset(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(2),
        ..area
    }
}

/// A `×` at the right end of a row that closes what the row heads. Its target
/// reaches a column past the glyph each way and a row down, for a fingertip.
fn close_button(frame: &mut Frame, app: &mut App, row: Rect) {
    let x = row.right().saturating_sub(3);
    app.hits
        .keys
        .push((Rect::new(x, row.y, 3, 2), KeyCode::Esc));
    frame.render_widget(Line::styled(" ×", ACCENT), Rect::new(x, row.y, 3, 1));
}

fn count(items: usize, noun: &str) -> String {
    let plural = match (items, noun.ends_with('s')) {
        (1, _) => "",
        (_, true) => "es",
        (_, false) => "s",
    };
    format!("{items} {noun}{plural}")
}

/// What the top row says about the shown section, counting what the search
/// and filter leave when they narrow it.
fn summary(app: &App) -> Option<String> {
    let narrowed =
        |shown: usize, total: usize| (shown != total).then(|| format!("{shown} of {total} · "));
    match app.section {
        Section::Docker => {
            let containers = app.docker.as_ref()?.as_ref().ok()?;
            let running = containers.iter().filter(|c| c.is_running_status()).count();
            let shown = narrowed(app.visible_containers().len(), containers.len());
            Some(format!("{}{running} running", shown.unwrap_or_default()))
        }
        Section::Disks => {
            let disks = app.disks.as_ref()?.as_ref().ok()?;
            let shown = narrowed(app.visible_disks().len(), disks.len());
            Some(format!(
                "{}{}",
                shown.unwrap_or_default(),
                count(disks.len(), "disk")
            ))
        }
        Section::Services => {
            let services = app.services.as_ref()?.as_ref().ok()?;
            let active = services.iter().filter(|s| s.is_running()).count();
            let shown = narrowed(app.visible_services().len(), services.len());
            Some(format!("{}{active} active", shown.unwrap_or_default()))
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

/// Section tabs, two columns apart: the shown one in the accent, the others
/// dimmed, like ramp's change scopes, with the summary at the right when it
/// fits. Each tab's target takes the gap beside it and the row below.
fn draw_header(frame: &mut Frame, app: &mut App, area: Rect) {
    let mut spans = Vec::new();
    let mut x = area.x;
    for section in Section::ALL {
        let title = section.title();
        let width = title.chars().count() as u16;
        if x + width > area.right() {
            break;
        }
        let target = Rect::new(x.saturating_sub(1), area.y, width + 2, 2);
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
    if let Some(summary) = summary(app) {
        let width = summary.chars().count() as u16;
        if x + width <= area.right() {
            let at = Rect::new(area.right() - width, area.y, width, 1);
            frame.render_widget(Line::styled(summary, MUTED), at);
        }
    }
}

/// The search field on the surface color, and the section's chips at its
/// right: the chosen one in the accent on a pill. When they don't all fit,
/// the chosen chip alone shows and a tap moves to the next.
fn draw_search(frame: &mut Frame, app: &mut App, area: Rect) {
    let chips: &[&str] = if app.logs.is_some() {
        &[]
    } else {
        search::chips(app.section)
    };
    let chosen = app.search[app.section.index()].chip;
    let all: u16 = chips.iter().map(|c| c.chars().count() as u16 + 3).sum();
    // The field keeps room for a short query.
    let compact = all + 22 > area.width;
    let width = if compact {
        chips
            .get(chosen)
            .map_or(0, |c| c.chars().count() as u16 + 3)
    } else {
        all
    };
    let [field, chip_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(width)]).areas(area);
    let mut x = chip_area.x + 1;
    for (index, chip) in chips.iter().enumerate() {
        if compact && index != chosen {
            continue;
        }
        let label = format!(" {chip} ");
        let at = Rect::new(x, area.y, label.chars().count() as u16, 1);
        let line = if index == chosen {
            Line::styled(label, app.palette.pill.patch(ACCENT))
        } else {
            Line::styled(label, MUTED)
        };
        frame.render_widget(line, at);
        if compact {
            app.hits.keys.push((at, KeyCode::Char('f')));
        } else {
            app.hits.chips.push((at, index));
        }
        x = at.right() + 1;
    }

    frame.render_widget(Block::new().style(app.palette.surface), field);
    app.hits.search = Some(field);
    let query = app.query().to_owned();
    let prompt = Span::styled(" / ", if app.typing { ACCENT } else { MUTED });
    let [prefix, input, clear] = Layout::horizontal([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(field);
    frame.render_widget(Line::from(prompt), prefix);
    let text = if query.is_empty() && !app.typing {
        let placeholder = match &app.logs {
            Some(_) => "search logs",
            None => search::placeholder(app.section),
        };
        Line::styled(placeholder, MUTED)
    } else {
        Line::raw(query.clone())
    };
    let width = text.width().min(usize::from(u16::MAX)) as u16;
    let scroll = if query.is_empty() {
        0
    } else {
        width.saturating_sub(input.width.saturating_sub(1))
    };
    frame.render_widget(Paragraph::new(text).scroll((0, scroll)), input);
    if app.typing && !input.is_empty() {
        let column = width.min(input.width.saturating_sub(1));
        frame.set_cursor_position(Position::new(input.x + column, input.y));
    }
    if !query.is_empty() {
        app.hits.keys.push((clear, KeyCode::Delete));
        frame.render_widget(Line::styled(" × ", MUTED), clear);
    }
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

/// Action labels without key hints, keeping the pane for content.
fn pill(app: &App, (_, label): Button) -> Line<'static> {
    Line::raw(format!(" {label} ")).style(app.palette.pill)
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
    let width = usize::from(width.max(1));
    text.chars().count().div_ceil(width).clamp(1, 3) as u16
}

/// The last action's progress or outcome, on its own lines at the bottom.
fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let Some((text, style)) = status_text(app) else {
        return;
    };
    frame.render_widget(
        Paragraph::new(text).style(style).wrap(Wrap { trim: true }),
        area,
    );
}

/// Logs cover the section under a heading in the accent, because they have
/// focus, showing the lines the search matches. They follow the newest line
/// until scrolled back.
fn draw_logs(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(logs) = &app.logs else { return };
    let (text, style) = match &logs.text {
        Ok(text) => {
            let lines: Vec<&str> = text
                .lines()
                .filter(|line| search::matches(&logs.query, &[line]))
                .collect();
            match (lines.is_empty(), logs.query.is_empty()) {
                (true, true) => ("No log lines.".to_owned(), MUTED),
                (true, false) => ("No lines match.".to_owned(), MUTED),
                _ => (lines.join("\n"), Style::new()),
            }
        }
        Err(error) => (error.clone(), ERROR),
    };
    let title = logs.title.clone();
    frame.render_widget(Clear, area);
    let [heading, body] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    frame.render_widget(Line::styled(title, ACCENT), heading);
    close_button(frame, app, heading);
    let paragraph = Paragraph::new(text).style(style).wrap(Wrap { trim: false });
    let lines = paragraph.line_count(body.width);
    let Some(logs) = &mut app.logs else { return };
    logs.max = lines
        .saturating_sub(usize::from(body.height))
        .min(usize::from(u16::MAX)) as u16;
    logs.offset = if logs.follow {
        logs.max
    } else {
        logs.offset.min(logs.max)
    };
    frame.render_widget(paragraph.scroll((logs.offset, 0)), body);
    app.hits.overlay = Some(area);
}

/// Packs items into as few lines of `width` as hold them, three columns
/// apart, never splitting an item across lines.
pub fn pack(items: Vec<Vec<Span<'static>>>, width: usize) -> Vec<Line<'static>> {
    const GAP: usize = 3;
    let mut lines: Vec<(usize, Vec<Span<'static>>)> = Vec::new();
    for item in items {
        let size: usize = item.iter().map(Span::width).sum();
        match lines.last_mut() {
            Some((used, spans)) if *used + GAP + size <= width => {
                spans.push(Span::raw(" ".repeat(GAP)));
                spans.extend(item);
                *used += GAP + size;
            }
            _ => lines.push((size, item)),
        }
    }
    lines
        .into_iter()
        .map(|(_, spans)| Line::from(spans))
        .collect()
}

/// A muted label and its value, one item of an expanded row's details.
pub fn detail(label: &str, value: impl Into<String>) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("{label} "), MUTED),
        Span::raw(value.into()),
    ]
}

/// Tables at least this wide put an expanded row's buttons beside its details.
const INLINE: u16 = 70;

/// The selected row's details and buttons, laid out for the table's width.
struct Expansion {
    lines: Vec<Line<'static>>,
    width: u16,
    detail_height: u16,
    /// Each button with its column offset, row and width.
    buttons: Vec<(Button, u16, u16, u16)>,
    height: u16,
}

impl Expansion {
    fn new(app: &App, area: Rect) -> Option<Self> {
        let available = area.height.saturating_sub(2);
        if !app.expanded || app.logs.is_some() || available == 0 {
            return None;
        }
        let items = match app.section {
            Section::Docker => docker::details(app)?,
            Section::Services => services::details(app)?,
            Section::Disks | Section::System => return None,
        };
        // Two columns in from the table's edge, under the row's text.
        let inner = area.width.saturating_sub(3).max(1);
        let sizes: Vec<(Button, u16)> = actions(app)
            .into_iter()
            .map(|button| (button, pill(app, button).width() as u16))
            .collect();
        let wrapped_height = |lines: &[Line<'static>], width: u16| {
            Paragraph::new(lines.to_vec())
                .wrap(Wrap { trim: false })
                .line_count(width)
                .min(usize::from(u16::MAX)) as u16
        };
        if area.width >= INLINE {
            let total: u16 = sizes.iter().map(|(_, w)| w + 1).sum();
            let width = inner.saturating_sub(total + 2).max(1);
            let lines = pack(items, usize::from(width));
            let mut x = inner.saturating_sub(total) + 1;
            let buttons = sizes
                .into_iter()
                .map(|(button, w)| {
                    let place = (button, x, 0, w);
                    x += w + 1;
                    place
                })
                .collect();
            let detail_height = wrapped_height(&lines, width).max(1).min(available);
            let height = (detail_height + 1).min(available);
            return Some(Self {
                lines,
                width,
                detail_height,
                buttons,
                height,
            });
        }
        let lines = pack(items, usize::from(inner));
        // Reserve the buttons before metadata, so the keyboard leaving fewer
        // rows never pushes the actions offscreen. Long metadata wraps.
        let (mut x, mut row) = (0, 0);
        let mut buttons = Vec::new();
        for (button, w) in sizes {
            if x > 0 && x + w > inner {
                (x, row) = (0, row + 2);
            }
            buttons.push((button, x, row, w.min(inner)));
            x += w + 1;
        }
        let button_height = row + 1;
        let detail_height =
            wrapped_height(&lines, inner).min(available.saturating_sub(button_height + 2));
        let gap = u16::from(available > button_height + detail_height);
        for (_, _, row, _) in &mut buttons {
            *row += detail_height + gap;
        }
        Some(Self {
            lines,
            width: inner,
            detail_height,
            buttons,
            height: (detail_height + gap + button_height + 1).min(available),
        })
    }

    fn draw(&self, frame: &mut Frame, app: &mut App, area: Rect) {
        frame.render_widget(Clear, area);
        frame.render_widget(Block::new().style(app.palette.surface), area);
        let inner = Rect {
            x: area.x + 2,
            width: area.width.saturating_sub(3),
            ..area
        };
        let metadata = Rect {
            width: self.width.min(inner.width),
            height: self.detail_height.min(inner.height),
            ..inner
        };
        frame.render_widget(
            Paragraph::new(self.lines.clone()).wrap(Wrap { trim: false }),
            metadata,
        );
        for &(button, x, row, width) in &self.buttons {
            let place = Rect::new(inner.x + x, inner.y + row, width, 1).intersection(area);
            if place.is_empty() {
                continue;
            }
            let target = if row == 0 {
                place
            } else {
                Rect {
                    y: place.y - 1,
                    height: 2,
                    ..place
                }
            };
            app.hits.keys.push((target, button.0));
            frame.render_widget(pill(app, button), place);
        }
    }
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

/// A name column as wide as its longest entry, up to `share` of the width,
/// so the other columns sit beside the names instead of at the far edge.
pub fn name_column(longest: usize, width: u16, share: f32) -> Constraint {
    let cap = (f32::from(width) * share) as u16;
    Constraint::Length((longest as u16).clamp(8, cap.max(8)))
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
        Some(Ok(rows)) if rows.is_empty() => {
            let filtered =
                app.section != Section::System && app.search[app.section.index()].chip != 0;
            let empty = if filtered || !app.query().is_empty() {
                "Nothing matches. Esc clears the search."
            } else {
                empty
            };
            return message(frame, area, empty, false);
        }
        Some(Ok(rows)) => rows,
    };
    let selectable = app.selectable();
    let count = rows.len();
    let expansion = Expansion::new(app, area);
    let extra = expansion.as_ref().map_or(0, |e| e.height);
    let state = &mut app.tables[app.section.index()];
    let selected = state.selected();
    let rows: Vec<Row> = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            if Some(index) == selected && extra > 0 {
                row.height(1 + extra)
            } else {
                row
            }
        })
        .collect();
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
    let offset = state.offset();
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    app.hits.body = Some(body);
    // Where each visible row landed, as the table laid them out from its offset.
    let mut y = body.y;
    let mut details = None;
    for index in offset..count {
        if y >= body.bottom() {
            break;
        }
        app.hits
            .rows
            .push((Rect::new(body.x, y, body.width, 1), index));
        y += 1;
        if Some(index) == selected && extra > 0 {
            details = Some(Rect::new(body.x, y, body.width, extra).intersection(body));
            y += extra;
        }
    }
    if let (Some(expansion), Some(area)) = (expansion, details) {
        expansion.draw(frame, app, area);
    }
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
