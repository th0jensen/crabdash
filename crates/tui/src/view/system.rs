//! CPU, memory and I/O activity with the busiest processes.
use super::{HIGHLIGHT, MUTED, message, table};
use crate::app::App;
use machines::resources::ResourceUsage;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Cell, LineGauge, Row, Sparkline},
};
use utils::disks::format_bytes;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let usage = match &app.usage {
        None => return message(frame, area, "Loading…", false),
        Some(Err(error)) => return message(frame, area, error, true),
        Some(Ok(usage)) => usage.clone(),
    };
    let [cpu, memory, swap, history, io, processes] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(u16::from(
            usage.swap.is_some_and(|swap| swap.total_bytes > 0),
        )),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(area);

    let cores = format!("{} cores", usage.logical_cpus);
    gauge(
        frame,
        cpu,
        "CPU",
        usage.cpu_percent.map(|p| p / 100.0),
        &cores,
    );
    let used = usage.memory.used_bytes();
    gauge(
        frame,
        memory,
        "Memory",
        Some(ratio(used, usage.memory.total_bytes)),
        &format!(
            "{} / {}",
            format_bytes(used),
            format_bytes(usage.memory.total_bytes)
        ),
    );
    if let Some(swap_sample) = usage.swap.filter(|swap| swap.total_bytes > 0) {
        let used = swap_sample.used_bytes();
        gauge(
            frame,
            swap,
            "Swap",
            Some(ratio(used, swap_sample.total_bytes)),
            &format!(
                "{} / {}",
                format_bytes(used),
                format_bytes(swap_sample.total_bytes)
            ),
        );
    }
    let history_data: Vec<u64> = app.cpu_history.iter().copied().collect();
    let start = history_data
        .len()
        .saturating_sub(usize::from(history.width));
    frame.render_widget(
        Sparkline::default()
            .data(&history_data[start..])
            .max(100)
            .style(Style::new().fg(HIGHLIGHT)),
        history,
    );
    frame.render_widget(activity(&usage), io);
    process_table(frame, app, processes, &usage);
}

fn ratio(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64
    }
}

fn gauge(frame: &mut Frame, area: Rect, label: &str, ratio: Option<f64>, detail: &str) {
    let ratio = ratio.unwrap_or_default().clamp(0.0, 1.0);
    let color = match ratio {
        r if r >= 0.9 => Color::Red,
        r if r >= 0.7 => Color::Yellow,
        _ => Color::Green,
    };
    let [bar, text] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(detail.len() as u16 + 1),
    ])
    .areas(area);
    frame.render_widget(
        LineGauge::default()
            .label(format!("{label:<7}{:>4.0}%", ratio * 100.0))
            .ratio(ratio)
            .filled_style(Style::new().fg(color))
            .unfilled_style(MUTED),
        bar,
    );
    frame.render_widget(Line::from(detail).style(MUTED).right_aligned(), text);
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

/// Load, the first GPU, and network and disk throughput.
fn activity(usage: &ResourceUsage) -> ratatui::text::Text<'static> {
    let mut first = Vec::new();
    if let Some([one, five, fifteen]) = usage.load_average {
        first.push(Span::styled("Load ", MUTED));
        first.push(Span::from(format!("{one:.2} {five:.2} {fifteen:.2}")));
    }
    if let Some(gpu) = usage.gpus.as_ref().and_then(|gpus| gpus.first()) {
        first.push(Span::styled(
            if first.is_empty() { "GPU " } else { "  GPU " },
            MUTED,
        ));
        first.push(Span::from(
            gpu.busy_percent
                .map_or_else(|| gpu.name.clone(), |busy| format!("{busy:.0}%")),
        ));
    }
    let network = usage.network.as_deref().unwrap_or_default();
    let disks = usage.disks.as_deref().unwrap_or_default();
    let received = sum(network.iter().map(|n| n.received_bytes_per_second));
    let sent = sum(network.iter().map(|n| n.sent_bytes_per_second));
    let read = sum(disks.iter().map(|d| d.read_bytes_per_second));
    let written = sum(disks.iter().map(|d| d.written_bytes_per_second));
    vec![
        Line::from(first),
        Line::from(vec![
            Span::styled("Net  ↓ ", MUTED),
            Span::from(rate(received)),
            Span::styled("  ↑ ", MUTED),
            Span::from(rate(sent)),
        ]),
        Line::from(vec![
            Span::styled("Disk r ", MUTED),
            Span::from(rate(read)),
            Span::styled("  w ", MUTED),
            Span::from(rate(written)),
        ]),
    ]
    .into()
}

pub fn uptime(seconds: f64) -> String {
    let minutes = (seconds / 60.0) as u64;
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match days {
        0 => format!("{hours}h {minutes}m"),
        days => format!("{days}d {hours}h"),
    }
}

fn process_table(frame: &mut Frame, app: &mut App, area: Rect, usage: &ResourceUsage) {
    let mut processes = usage.processes.clone().unwrap_or_default();
    processes.sort_by(|a, b| {
        b.cpu_percent
            .unwrap_or(-1.0)
            .total_cmp(&a.cpu_percent.unwrap_or(-1.0))
    });
    let rows = processes
        .into_iter()
        .map(|process| {
            Row::new([
                Cell::from(process.name),
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
    table(
        frame,
        app,
        area,
        Some(Ok(rows)),
        Row::new(["Process", "CPU", "Memory", "PID"]),
        &[
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Length(9),
            Constraint::Length(7),
        ],
        "No processes.",
    );
}
