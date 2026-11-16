//! CPU, cores, memory, GPUs and I/O beside or above the busiest processes.
//! The layout follows the pane: a wide pane puts the meters in a column left
//! of the processes, a narrow one stacks them, and each part grows or gives
//! way as rows allow, with the processes keeping enough rows to be useful.
use super::{ACCENT, HIGHLIGHT, MUTED, message, pack, table};
use crate::app::App;
use machines::resources::ResourceUsage;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, LineGauge, Row, Sparkline},
};
use utils::disks::format_bytes;

/// Process rows the stacked layout keeps, with their header.
const PROCESSES: u16 = 7;
/// A core's cell in the grid: its number, bar and percentage.
const CORE_CELL: u16 = 13;
/// Panes at least this wide put the meters beside the processes.
const SIDE_BY_SIDE: u16 = 100;
/// The CPU history's tallest height.
const CHART: u16 = 8;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let usage = match &app.usage {
        None => return message(frame, area, "Loading…", false),
        Some(Err(error)) => return message(frame, area, error, true),
        Some(Ok(usage)) => usage.clone(),
    };
    let history: Vec<u64> = app.cpu_history.iter().copied().collect();
    if area.width >= SIDE_BY_SIDE && area.height >= 10 {
        let meters = (area.width * 2 / 5).clamp(44, 72);
        let [left, _, right] = Layout::horizontal([
            Constraint::Length(meters),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .areas(area);
        let plan = Plan::new(&usage, left.width, left.height);
        // The chart stops growing at a readable height; the busiest
        // interfaces and disks take what is left.
        let chart = left.height.saturating_sub(plan.fixed).min(CHART);
        let [meters, _, rates] = Layout::vertical([
            Constraint::Length(plan.fixed + chart),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(left);
        draw_meters(frame, meters, &usage, &history, &plan, meters.height);
        draw_rates(frame, rates, &usage);
        return process_table(frame, app, right);
    }
    let reserved = PROCESSES.min(area.height / 2);
    let plan = Plan::new(&usage, area.width, area.height.saturating_sub(reserved));
    let rest = area.height.saturating_sub(plan.fixed);
    // Tall panes give the CPU history a share of what the processes leave.
    let history_rows = match rest.saturating_sub(PROCESSES) {
        spare if spare >= 4 => (spare / 3).clamp(2, CHART),
        _ => 0,
    };
    let [meters, processes] = Layout::vertical([
        Constraint::Length(plan.fixed + history_rows),
        Constraint::Fill(1),
    ])
    .areas(area);
    draw_meters(frame, meters, &usage, &history, &plan, meters.height);
    process_table(frame, app, processes);
}

/// How the cores show: a grid of labelled bars, a strip of one cell each, or
/// not at all when the rows are needed elsewhere.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Cores {
    Grid { columns: u16, rows: u16 },
    Strip { rows: u16 },
    Hidden,
}

impl Cores {
    fn rows(self) -> u16 {
        match self {
            Self::Grid { rows, .. } => rows + 1,
            Self::Strip { rows } => rows,
            Self::Hidden => 0,
        }
    }
}

/// The meters' rows, chosen for the width and the rows they may take.
struct Plan {
    gauges: u16,
    cores: Cores,
    activity: Vec<Line<'static>>,
    /// Rows before the CPU history.
    fixed: u16,
}

impl Plan {
    fn new(usage: &ResourceUsage, width: u16, budget: u16) -> Self {
        let gauges = (2
            + u16::from(swap(usage).is_some())
            + usage.gpus.as_ref().map_or(0, |g| g.len().min(2) as u16))
        .min(budget);
        let mut activity = activity(usage, width);
        activity.truncate(usize::from(budget.saturating_sub(gauges)));
        let base = (gauges + activity.len() as u16 + 1).min(budget);
        let cores = core_layout(usage.cores.len() as u16, width, budget.saturating_sub(base));
        Self {
            gauges,
            cores,
            fixed: base + cores.rows(),
            activity,
        }
    }
}

/// The largest core layout that fits `rows`: a grid when it fits, else a strip.
fn core_layout(cores: u16, width: u16, rows: u16) -> Cores {
    if cores <= 1 {
        return Cores::Hidden;
    }
    let columns = ((width + 2) / (CORE_CELL + 2)).max(1);
    let grid = cores.div_ceil(columns);
    if grid < rows {
        return Cores::Grid {
            columns,
            rows: grid,
        };
    }
    let strip = cores.div_ceil(width.saturating_sub(7).max(1));
    if strip <= rows {
        Cores::Strip { rows: strip }
    } else {
        Cores::Hidden
    }
}

fn draw_meters(
    frame: &mut Frame,
    area: Rect,
    usage: &ResourceUsage,
    history: &[u64],
    plan: &Plan,
    height: u16,
) {
    let [gauges, cores, activity, _, chart] = Layout::vertical([
        Constraint::Length(plan.gauges),
        Constraint::Length(plan.cores.rows()),
        Constraint::Length(plan.activity.len() as u16),
        Constraint::Length(1),
        Constraint::Length(height.saturating_sub(plan.fixed)),
    ])
    .areas(area);
    let rows =
        Layout::vertical(vec![Constraint::Length(1); usize::from(plan.gauges)]).split(gauges);
    let mut rows = rows.iter().copied();
    let mut next = || rows.next().unwrap_or_default();
    gauge(
        frame,
        next(),
        "CPU",
        usage.cpu_percent.map(|p| p / 100.0),
        &format!("{} cores", usage.logical_cpus),
    );
    let used = usage.memory.used_bytes();
    gauge(
        frame,
        next(),
        "Memory",
        Some(ratio(used, usage.memory.total_bytes)),
        &of(used, usage.memory.total_bytes),
    );
    if let Some((used, total)) = swap(usage) {
        gauge(
            frame,
            next(),
            "Swap",
            Some(ratio(used, total)),
            &of(used, total),
        );
    }
    for gpu in usage.gpus.iter().flatten().take(2) {
        let mut detail = Vec::new();
        if let (Some(used), Some(total)) = (gpu.memory_used_bytes, gpu.memory_total_bytes) {
            detail.push(of(used, total));
        }
        if let Some(celsius) = gpu.temperature_celsius {
            detail.push(format!("{celsius:.0}°C"));
        }
        if detail.is_empty() {
            detail.push(gpu.name.clone());
        }
        gauge(
            frame,
            next(),
            "GPU",
            gpu.busy_percent.map(|p| p / 100.0),
            &detail.join(" · "),
        );
    }
    draw_cores(frame, cores, usage, plan.cores);
    frame.render_widget(ratatui::text::Text::from(plan.activity.clone()), activity);
    if chart.height > 0 {
        let start = history.len().saturating_sub(usize::from(chart.width));
        frame.render_widget(
            Sparkline::default()
                .data(&history[start..])
                .max(100)
                .style(Style::new().fg(HIGHLIGHT)),
            chart,
        );
    }
}

fn level(ratio: f64) -> Color {
    match ratio {
        r if r >= 0.9 => Color::Red,
        r if r >= 0.7 => Color::Yellow,
        _ => Color::Green,
    }
}

fn draw_cores(frame: &mut Frame, area: Rect, usage: &ResourceUsage, cores: Cores) {
    let percents: Vec<f64> = usage
        .cores
        .iter()
        .map(|core| core.percent.unwrap_or_default() / 100.0)
        .collect();
    match cores {
        Cores::Hidden => {}
        Cores::Strip { .. } => {
            const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            let width = usize::from(area.width.saturating_sub(7).max(1));
            let lines: Vec<Line> = percents
                .chunks(width)
                .enumerate()
                .map(|(row, chunk)| {
                    let label = if row == 0 { "Cores  " } else { "       " };
                    let mut spans = vec![Span::styled(label, MUTED)];
                    spans.extend(chunk.iter().map(|&ratio| {
                        let bar = BARS[((ratio.clamp(0.0, 1.0) * 7.0).round()) as usize];
                        Span::styled(bar.to_string(), Style::new().fg(level(ratio)))
                    }));
                    Line::from(spans)
                })
                .collect();
            frame.render_widget(ratatui::text::Text::from(lines), area);
        }
        Cores::Grid { columns, rows } => {
            let lines =
                Layout::vertical(vec![Constraint::Length(1); usize::from(rows)]).split(Rect {
                    height: rows,
                    ..area
                });
            for (row, line) in lines.iter().enumerate() {
                let cells = Layout::horizontal(vec![Constraint::Fill(1); usize::from(columns)])
                    .spacing(2)
                    .split(*line);
                for (column, cell) in cells.iter().enumerate() {
                    // Down each column, then across, as top numbers cores.
                    let index = column * usize::from(rows) + row;
                    let Some(&ratio) = percents.get(index) else {
                        continue;
                    };
                    frame.render_widget(
                        LineGauge::default()
                            .label(Line::from(vec![
                                Span::styled(format!("{index:>2} "), MUTED),
                                Span::raw(format!("{:>3.0}%", ratio * 100.0)),
                            ]))
                            .ratio(ratio.clamp(0.0, 1.0))
                            .filled_style(Style::new().fg(level(ratio)))
                            .unfilled_style(MUTED),
                        *cell,
                    );
                }
            }
        }
    }
}

fn swap(usage: &ResourceUsage) -> Option<(u64, u64)> {
    usage
        .swap
        .filter(|swap| swap.total_bytes > 0)
        .map(|swap| (swap.used_bytes(), swap.total_bytes))
}

fn of(used: u64, total: u64) -> String {
    format!("{} / {}", format_bytes(used), format_bytes(total))
}

fn ratio(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64
    }
}

/// A labelled bar with its detail at the right, which gives way first when
/// the bar would get too short to read.
fn gauge(frame: &mut Frame, area: Rect, label: &str, ratio: Option<f64>, detail: &str) {
    let detail_width = detail.chars().count() as u16 + 1;
    let detail_width = if area.width >= detail_width + 18 {
        detail_width
    } else {
        0
    };
    let [bar, text] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(detail_width)]).areas(area);
    let percent = ratio.map_or_else(|| "   —".to_owned(), |r| format!("{:>3.0}%", r * 100.0));
    let ratio = ratio.unwrap_or_default().clamp(0.0, 1.0);
    frame.render_widget(
        LineGauge::default()
            .label(format!("{label:<7}{percent}"))
            .ratio(ratio)
            .filled_style(Style::new().fg(level(ratio)))
            .unfilled_style(MUTED),
        bar,
    );
    frame.render_widget(
        Line::from(detail.to_owned()).style(MUTED).right_aligned(),
        text,
    );
}

fn rate(value: Option<f64>) -> String {
    value.map_or_else(
        || "—".into(),
        |value| format!("{}/s", format_bytes(value as u64)),
    )
}

fn sum(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    values.fold(None, |total, value| match (total, value) {
        (None, value) => value,
        (total, None) => total,
        (Some(total), Some(value)) => Some(total + value),
    })
}

/// Load and network and disk throughput, packed into as few lines as the
/// width allows, never splitting one reading across lines.
fn activity(usage: &ResourceUsage, width: u16) -> Vec<Line<'static>> {
    let mut items: Vec<Vec<Span<'static>>> = Vec::new();
    if let Some([one, five, fifteen]) = usage.load_average {
        items.push(vec![
            Span::styled("Load ", MUTED),
            Span::from(format!("{one:.2} {five:.2} {fifteen:.2}")),
        ]);
    }
    let network = usage.network.as_deref().unwrap_or_default();
    let disks = usage.disks.as_deref().unwrap_or_default();
    items.push(vec![
        Span::styled("Net ↓ ", MUTED),
        Span::from(rate(sum(network
            .iter()
            .map(|n| n.received_bytes_per_second)))),
        Span::styled(" ↑ ", MUTED),
        Span::from(rate(sum(network.iter().map(|n| n.sent_bytes_per_second)))),
    ]);
    items.push(vec![
        Span::styled("Disk r ", MUTED),
        Span::from(rate(sum(disks.iter().map(|d| d.read_bytes_per_second)))),
        Span::styled(" w ", MUTED),
        Span::from(rate(sum(disks.iter().map(|d| d.written_bytes_per_second)))),
    ]);
    pack(items, usize::from(width))
}

/// Each network interface and disk with its throughput, busiest first, as
/// many as fit.
fn draw_rates(frame: &mut Frame, area: Rect, usage: &ResourceUsage) {
    let value = |rate: Option<f64>| rate.unwrap_or_default();
    let mut rows: Vec<(f64, Line<'static>)> = Vec::new();
    for network in usage.network.iter().flatten() {
        let (down, up) = (
            network.received_bytes_per_second,
            network.sent_bytes_per_second,
        );
        rows.push((
            value(down) + value(up),
            rate_line(&network.id, "↓", down, "↑", up),
        ));
    }
    for disk in usage.disks.iter().flatten() {
        let (read, write) = (disk.read_bytes_per_second, disk.written_bytes_per_second);
        rows.push((
            value(read) + value(write),
            rate_line(&disk.id, "r", read, "w", write),
        ));
    }
    if rows.is_empty() || area.height < 2 {
        return;
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut lines = vec![Line::styled("Interfaces and disks", MUTED)];
    lines.extend(rows.into_iter().map(|(_, line)| line));
    frame.render_widget(ratatui::text::Text::from(lines), area);
}

fn rate_line(
    name: &str,
    first: &str,
    a: Option<f64>,
    second: &str,
    b: Option<f64>,
) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("{name:<14} ")),
        Span::styled(format!("{first} "), MUTED),
        Span::raw(format!("{:<12}", rate(a))),
        Span::styled(format!("{second} "), MUTED),
        Span::raw(rate(b)),
    ])
}

pub fn uptime(seconds: f64) -> String {
    let minutes = (seconds / 60.0) as u64;
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match days {
        0 => format!("{hours}h {minutes}m"),
        days => format!("{days}d {hours}h"),
    }
}

/// Processes in the chosen order, the sorted column's heading in the accent.
/// Wider panes add the user.
fn process_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let sort = app.search[crate::section::Section::System.index()].chip;
    let with_user = area.width >= 70;
    let rows = app
        .visible_processes()
        .into_iter()
        .map(|process| {
            Row::new([
                Cell::from(process.name),
                Cell::from(process.user.unwrap_or_default()).style(MUTED),
                Cell::from(
                    process
                        .cpu_percent
                        .map_or_else(|| "—".into(), |cpu| format!("{cpu:.1}%")),
                ),
                Cell::from(process.memory_bytes.map(format_bytes).unwrap_or_default()),
                Cell::from(process.pid.to_string()).style(MUTED),
            ])
        })
        .collect();
    let sorted = ACCENT.remove_modifier(Modifier::DIM);
    let heading = |name: &'static str, column: usize| {
        let cell = Cell::from(name);
        if column == sort {
            cell.style(sorted)
        } else {
            cell
        }
    };
    table(
        frame,
        app,
        area,
        Some(Ok(rows)),
        Row::new([
            heading("Process", 2),
            Cell::from("User"),
            heading("CPU", 0),
            heading("Memory", 1),
            Cell::from("PID"),
        ]),
        &[
            Constraint::Fill(1),
            Constraint::Length(if with_user { 10 } else { 0 }),
            Constraint::Length(6),
            Constraint::Length(9),
            Constraint::Length(7),
        ],
        "No processes.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use machines::resources::{CpuUsage, GpuSample, MemorySample, ProcessUsage};
    use ratatui::{Terminal, backend::TestBackend};

    fn sample() -> ResourceUsage {
        ResourceUsage {
            cpu_percent: Some(37.0),
            cores: (0..8)
                .map(|i| CpuUsage {
                    name: format!("cpu{i}"),
                    percent: Some(f64::from(i) * 10.0),
                })
                .collect(),
            logical_cpus: 8,
            memory: MemorySample {
                total_bytes: 16_000_000_000,
                available_bytes: 4_000_000_000,
                estimated: false,
            },
            swap: None,
            load_average: Some([1.23, 2.34, 0.45]),
            uptime_seconds: 90061.0,
            processes: Some(vec![ProcessUsage {
                pid: 502,
                start_id: "start".into(),
                name: "renderer".into(),
                user: Some("user".into()),
                memory_bytes: Some(2_000_000_000),
                cpu_percent: Some(31.2),
            }]),
            process_count: Some(1),
            processes_truncated: false,
            network: None,
            disks: None,
            gpus: Some(vec![GpuSample {
                id: "gpu0".into(),
                name: "NVIDIA RTX 4070".into(),
                vendor: "NVIDIA".into(),
                driver: None,
                busy_percent: Some(88.0),
                memory_used_bytes: Some(3_000_000_000),
                memory_total_bytes: Some(8_000_000_000),
                temperature_celsius: Some(53.0),
            }]),
        }
    }

    #[test]
    fn meters_and_processes_reflow_together_without_losing_readings() -> anyhow::Result<()> {
        let mut app = App::default();
        app.section = crate::section::Section::System;
        app.usage = Some(Ok(sample()));
        let mut terminal = Terminal::new(TestBackend::new(130, 32))?;
        for (width, height) in [(130, 32), (46, 30), (46, 12), (130, 32)] {
            terminal.backend_mut().resize(width, height);
            terminal.draw(|frame| super::super::draw(frame, &mut app))?;
            let lines: Vec<String> = terminal
                .backend()
                .buffer()
                .content
                .chunks(usize::from(width))
                .map(|row| row.iter().map(|cell| cell.symbol()).collect())
                .collect();
            let screen = lines.join("\n");
            for value in ["37%", "75%", "88%", "renderer", "31.2%", "2.0 GB"] {
                assert!(
                    screen.contains(value),
                    "{value} missing at {width}×{height}:\n{screen}"
                );
            }
            let cpu = lines.iter().position(|line| line.contains("CPU "));
            let processes = lines.iter().position(|line| line.contains("Process "));
            if width >= 100 {
                assert_eq!(cpu, processes, "wide panes place meters beside processes");
            } else {
                assert!(cpu < processes, "narrow panes stack meters above processes");
            }
        }
        Ok(())
    }
}
