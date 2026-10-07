//! Responsive resource mosaic: the pane's width decides the arrangement.
use super::{
    HistoryPoint, MachineState, ScalarPoint,
    chart::{self, Series},
    sum_rates,
};
use crate::{
    app::Crabdash,
    components::{
        common::{clipped_text, control_tooltip, surface_button},
        style,
    },
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use machines::resources::{GpuSample, ResourceUsage};

fn percent(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(|| "Sampling…".into(), |value| format!("{value:.1}%"))
}
pub(super) fn bytes(value: u64) -> String {
    let mut value = value as f64;
    let mut unit = "B";
    for next in ["KiB", "MiB", "GiB", "TiB"] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    format!("{value:.1} {unit}")
}
fn rate(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or_else(
            || "Sampling…".into(),
            |value| format!("{}/s", bytes(value.min(u64::MAX as f64) as u64)),
        )
}
fn uptime(seconds: f64) -> String {
    let minutes = (seconds.max(0.0) / 60.0) as u64;
    let days = minutes / 1440;
    let hours = minutes % 1440 / 60;
    if days > 0 {
        format!("{days}d {hours}h {}m", minutes % 60)
    } else {
        format!("{hours}h {}m", minutes % 60)
    }
}
fn history_series(
    state: &MachineState,
    value: impl Fn(&HistoryPoint) -> Option<f64>,
    label: Option<&'static str>,
    color: u32,
    dashed: bool,
) -> Series {
    Series {
        label,
        samples: state
            .history
            .iter()
            .map(|point| ScalarPoint {
                captured_at: point.captured_at,
                value: value(point),
            })
            .collect(),
        color,
        dashed,
    }
}
pub(super) fn card() -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex_none()
        .p(rems(12.0 / 16.0))
        .bg(rgb(style::SURFACE))
        .border_1()
        .border_color(rgb(style::BORDER))
        .rounded(px(style::CARD_RADIUS))
        .flex()
        .flex_col()
        .gap(rems(10.0 / 16.0))
}
pub(super) fn heading(name: &str, value: String, detail: String) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_start()
        .justify_between()
        .gap(rems(8.0 / 16.0))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(clipped_text(name.to_string()).font_weight(FontWeight::SEMIBOLD))
                .when(!detail.is_empty(), |this| {
                    this.child(
                        clipped_text(detail)
                            .text_size(rems(style::META / 16.0))
                            .text_color(rgb(style::TEXT_MUTED)),
                    )
                }),
        )
        .when(!value.is_empty(), |this| {
            this.child(
                div()
                    .flex_none()
                    .text_size(rems(style::TEXT / 16.0))
                    .child(value),
            )
        })
}
fn fact(label: &str, value: String) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .gap(rems(8.0 / 16.0))
        .child(
            clipped_text(label.to_string())
                .w(rems(88.0 / 16.0))
                .flex_none()
                .text_color(rgb(style::TEXT_MUTED)),
        )
        .child(clipped_text(value).flex_1())
}
fn machine_fact(label: &str, value: String) -> Stateful<Div> {
    let tooltip = format!("{label}: {value}");
    fact(label, value)
        .id(SharedString::from(format!("system-machine-{label}")))
        .tooltip(move |_, cx| control_tooltip(tooltip.clone(), cx))
}
fn meter(value: f64) -> Div {
    div()
        .w_full()
        .h(px(3.0))
        .rounded(px(2.0))
        .bg(rgb(style::BORDER))
        .child(
            div()
                .h_full()
                .w(relative(value.clamp(0.0, 100.0) as f32 / 100.0))
                .rounded(px(2.0))
                .bg(rgb(style::TAB_INDICATOR)),
        )
}
fn cpu_card(state: &MachineState, usage: &ResourceUsage) -> Div {
    card()
        .child(heading(
            "CPU",
            percent(usage.cpu_percent),
            format!("{} logical processors", usage.logical_cpus),
        ))
        .child(chart::render(
            vec![history_series(
                state,
                |point| point.cpu,
                None,
                style::TEXT_PRIMARY,
                false,
            )],
            Some(100.0),
        ))
        .when(!usage.cores.is_empty(), |this| {
            this.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(rems(10.0 / 16.0))
                    .text_size(rems(style::META / 16.0))
                    .children(usage.cores.iter().map(|core| {
                        div()
                            .w(rems(104.0 / 16.0))
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_between()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .flex_none()
                                            .whitespace_nowrap()
                                            .text_color(rgb(style::TEXT_MUTED))
                                            .child(format!("CPU {}", core.name)),
                                    )
                                    .child(percent(core.percent)),
                            )
                            .child(meter(core.percent.unwrap_or(0.0)))
                    })),
            )
        })
        .child(fact("Uptime", uptime(usage.uptime_seconds)))
        .when_some(usage.load_average, |this, load| {
            this.child(fact(
                "Load",
                format!("{:.2} · {:.2} · {:.2}", load[0], load[1], load[2]),
            ))
        })
}
fn memory_card(state: &MachineState, usage: &ResourceUsage) -> Div {
    card()
        .child(heading(
            "Memory",
            percent(Some(usage.memory.used_percent())),
            format!(
                "{} / {} used",
                bytes(usage.memory.used_bytes()),
                bytes(usage.memory.total_bytes)
            ),
        ))
        .child(chart::render(
            vec![history_series(
                state,
                |point| point.memory,
                None,
                style::TEXT_MUTED,
                false,
            )],
            Some(100.0),
        ))
        .child(fact(
            if usage.memory.estimated {
                "Available ≈"
            } else {
                "Available"
            },
            bytes(usage.memory.available_bytes),
        ))
        .when_some(usage.swap, |this, swap| {
            this.child(fact(
                "Swap",
                format!(
                    "{} / {} used",
                    bytes(swap.used_bytes()),
                    bytes(swap.total_bytes)
                ),
            ))
        })
}
fn io_row(
    id: &str,
    read: Option<f64>,
    write: Option<f64>,
    read_label: &str,
    write_label: &str,
) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .text_size(rems(style::META / 16.0))
        .child(clipped_text(id.to_string()).text_color(rgb(style::TEXT_PRIMARY)))
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_wrap()
                .gap(rems(10.0 / 16.0))
                .child(
                    clipped_text(format!("{read_label} {}", rate(read)))
                        .max_w_full()
                        .text_color(rgb(style::TEXT_PRIMARY)),
                )
                .child(
                    clipped_text(format!("{write_label} {}", rate(write)))
                        .max_w_full()
                        .text_color(rgb(style::TEXT_MUTED)),
                ),
        )
}
fn network_card(state: &MachineState, usage: &ResourceUsage) -> Div {
    let Some(interfaces) = &usage.network else {
        return card().child(heading(
            "Network",
            "Unavailable".into(),
            "Network counters are not available on this machine.".into(),
        ));
    };
    if interfaces.is_empty() {
        return card().child(heading(
            "Network",
            "Unavailable".into(),
            "No network interfaces were reported.".into(),
        ));
    }
    card()
        .child(heading(
            "Network",
            String::new(),
            format!(
                "{} {} · receive / send",
                interfaces.len(),
                if interfaces.len() == 1 {
                    "interface"
                } else {
                    "interfaces"
                }
            ),
        ))
        .child(fact(
            "Receive",
            rate(sum_rates(
                interfaces
                    .iter()
                    .map(|interface| interface.received_bytes_per_second),
            )),
        ))
        .child(fact(
            "Send",
            rate(sum_rates(
                interfaces
                    .iter()
                    .map(|interface| interface.sent_bytes_per_second),
            )),
        ))
        .child(chart::render(
            vec![
                history_series(
                    state,
                    |point| point.network_rx,
                    Some("Receive"),
                    style::TEXT_PRIMARY,
                    false,
                ),
                history_series(
                    state,
                    |point| point.network_tx,
                    Some("Send"),
                    style::TEXT_MUTED,
                    true,
                ),
            ],
            None,
        ))
        .children(interfaces.iter().map(|interface| {
            io_row(
                &interface.id,
                interface.received_bytes_per_second,
                interface.sent_bytes_per_second,
                "↓",
                "↑",
            )
        }))
}
fn disk_card(state: &MachineState, usage: &ResourceUsage) -> Div {
    let Some(disks) = &usage.disks else {
        return card().child(heading(
            "Disk I/O",
            "Unavailable".into(),
            "Disk counters are not available on this machine.".into(),
        ));
    };
    if disks.is_empty() {
        return card().child(heading(
            "Disk I/O",
            "Unavailable".into(),
            "No disk devices were reported.".into(),
        ));
    }
    card()
        .child(heading(
            "Disk I/O",
            String::new(),
            format!(
                "{} {} · read / write",
                disks.len(),
                if disks.len() == 1 {
                    "device"
                } else {
                    "devices"
                }
            ),
        ))
        .child(fact(
            "Read",
            rate(sum_rates(
                disks.iter().map(|disk| disk.read_bytes_per_second),
            )),
        ))
        .child(fact(
            "Write",
            rate(sum_rates(
                disks.iter().map(|disk| disk.written_bytes_per_second),
            )),
        ))
        .child(chart::render(
            vec![
                history_series(
                    state,
                    |point| point.disk_read,
                    Some("Read"),
                    style::TEXT_PRIMARY,
                    false,
                ),
                history_series(
                    state,
                    |point| point.disk_write,
                    Some("Write"),
                    style::TEXT_MUTED,
                    true,
                ),
            ],
            None,
        ))
        .children(disks.iter().map(|disk| {
            io_row(
                &disk.id,
                disk.read_bytes_per_second,
                disk.written_bytes_per_second,
                "Read",
                "Write",
            )
        }))
}
fn gpu_card(state: &MachineState, gpu: &GpuSample) -> Div {
    let detail = [Some(gpu.vendor.as_str()), gpu.driver.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    card()
        .child(heading(
            if gpu.name.is_empty() {
                &gpu.id
            } else {
                &gpu.name
            },
            String::new(),
            detail,
        ))
        .child(
            fact("Device", gpu.id.clone())
                .id(SharedString::from(format!("gpu-device-{}", gpu.id)))
                .tooltip({
                    let id = gpu.id.clone();
                    move |_, cx| control_tooltip(format!("Device ID: {id}"), cx)
                }),
        )
        .child(fact(
            "GPU usage",
            gpu.busy_percent
                .map_or_else(|| "Unavailable".into(), |value| percent(Some(value))),
        ))
        .when_some(gpu.busy_percent, |this, value| this.child(meter(value)))
        .when_some(
            state.gpu_history.get(&gpu.id).filter(|history| {
                history
                    .iter()
                    .any(|point| point.value.is_some_and(|value| value.is_finite()))
            }),
            |this, history| {
                this.child(chart::render(
                    vec![Series {
                        label: None,
                        samples: history.iter().copied().collect(),
                        color: style::TEXT_PRIMARY,
                        dashed: false,
                    }],
                    Some(100.0),
                ))
            },
        )
        .when_some(gpu.memory_used_bytes, |this, used| {
            this.child(fact(
                "VRAM used",
                gpu.memory_total_bytes.map_or_else(
                    || bytes(used),
                    |total| format!("{} / {}", bytes(used), bytes(total)),
                ),
            ))
        })
        .when(gpu.memory_used_bytes.is_none(), |this| {
            this.when_some(gpu.memory_total_bytes, |this, total| {
                this.child(fact("VRAM", bytes(total)))
            })
        })
        .when_some(gpu.temperature_celsius, |this, temperature| {
            this.child(fact("Temperature", format!("{temperature:.1} °C")))
        })
        .when(
            gpu.busy_percent.is_none()
                && gpu.memory_used_bytes.is_none()
                && gpu.temperature_celsius.is_none(),
            |this| {
                this.child(
                    div()
                        .text_size(rems(style::META / 16.0))
                        .text_color(rgb(style::TEXT_MUTED))
                        .child("Utilization telemetry is unsupported by this adapter or driver."),
                )
            },
        )
}
pub(crate) fn render(
    app: &Crabdash,
    window: &Window,
    cx: &mut Context<Crabdash>,
    pane_width: Pixels,
) -> Stateful<Div> {
    let scale = f32::from(window.rem_size()) / 16.0;
    let width = f32::from(pane_width) / scale.max(0.01);
    let columns = if width >= 1000.0 {
        3
    } else if width >= 600.0 {
        2
    } else {
        1
    };
    let machine = app.selected_machine();
    let state = app.system.machines.get(&machine.uuid);
    let info = &machine.system_info;
    let distribution = info.distribution.as_ref().map_or_else(
        || info.platform_label().to_string(),
        |distribution| distribution.pretty_name.clone(),
    );
    let status = super::status::current(
        state,
        std::time::Instant::now(),
        app.preferences.system_refresh_interval(),
    );
    let mut mosaic = div()
        .w_full()
        .min_w_0()
        .grid()
        .grid_cols(columns)
        .gap(rems(12.0 / 16.0))
        .items_start();
    if let Some(state) = state
        && let Some(usage) = &state.usage
    {
        let mut cpu = cpu_card(state, usage).col_span(if columns == 3 { 2 } else { 1 });
        let mut memory = memory_card(state, usage);
        if columns > 1 {
            // Auto-height grid cards stretch only in this shared top row.
            cpu.style().align_self = Some(AlignSelf::Stretch);
            memory.style().align_self = Some(AlignSelf::Stretch);
        }
        mosaic = mosaic
            .child(cpu)
            .child(memory)
            .child(network_card(state, usage))
            .child(disk_card(state, usage));
        let machine = card()
            .child(heading("Machine", String::new(), String::new()))
            .child(machine_fact("Name", info.machine_name.clone()))
            .child(machine_fact("Platform", distribution))
            .child(machine_fact("Kernel", info.os_version.clone()))
            .child(machine_fact("Architecture", info.arch.clone()));
        let graphics: Vec<Div> = match &usage.gpus {
            Some(gpus) if !gpus.is_empty() => gpus.iter().map(|gpu| gpu_card(state, gpu)).collect(),
            Some(_) => vec![card().child(heading(
                "Graphics",
                "Unavailable".into(),
                "No graphics adapters were detected.".into(),
            ))],
            None => vec![card().child(heading(
                "Graphics",
                "Unavailable".into(),
                "GPU probing is unsupported on this machine.".into(),
            ))],
        };
        if columns == 3 {
            // Keep compact machine facts and graphics in the third column,
            // rather than allocating a separate mostly-empty GPU grid row.
            mosaic = mosaic.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(rems(12.0 / 16.0))
                    .child(machine)
                    .children(graphics),
            );
        } else {
            mosaic = mosaic.child(machine).children(graphics);
        }
        mosaic = mosaic.child(super::processes::render(app, usage, width, cx).col_span_full());
    } else {
        mosaic = mosaic.child(
            card()
                .col_span_full()
                .text_color(rgb(style::TEXT_MUTED))
                .child(status.empty_message()),
        );
    }
    let toolbar = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .text_size(rems(style::META / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child(status.label()),
        )
        .child(
            surface_button("system-refresh", Some(Icon::RefreshCw), None)
                .tooltip({
                    let seconds = app.preferences.system_refresh_seconds;
                    move |_, cx| {
                        control_tooltip(
                            format!(
                                "Refresh resources · live sampling runs every {seconds} seconds"
                            ),
                            cx,
                        )
                    }
                })
                .on_click(cx.listener(|app, _, _, cx| app.refresh_system_resources(cx))),
        );
    let content = div()
        .w(pane_width)
        .min_w_0()
        .flex()
        .flex_col()
        .gap(rems(12.0 / 16.0))
        .pb(px(16.0))
        .child(toolbar)
        .when_some(
            state.and_then(|state| state.error.as_ref()),
            |this, error| this.child(card().text_color(rgb(style::DANGER)).child(error.clone())),
        )
        .child(mosaic)
        .child(
            div()
                .text_size(rems(style::META / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child(
                    "Recent samples · rates require two samples · used memory = total − available",
                ),
        );
    div()
        .id("system-resources")
        .w(pane_width)
        .h_full()
        .min_w_0()
        .min_h_0()
        .overflow_y_scroll()
        .text_color(rgb(style::TEXT_PRIMARY))
        .text_size(rems(style::TEXT / 16.0))
        .child(content)
}

#[cfg(test)]
mod tests {
    use super::{bytes, percent, rate, sum_rates, uptime};
    #[test]
    fn readable_units_and_missing_rates_are_explicit() {
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
        assert_eq!(percent(None), "Sampling…");
        assert_eq!(percent(Some(f64::NAN)), "Sampling…");
        assert_eq!(rate(Some(1024.0)), "1.0 KiB/s");
        assert_eq!(rate(Some(-1.0)), "Sampling…");
        assert_eq!(sum_rates([Some(1.0), None, Some(2.0)].into_iter()), None);
        assert_eq!(sum_rates([Some(1.0), Some(2.0)].into_iter()), Some(3.0));
        assert_eq!(sum_rates([None, None].into_iter()), None);
        assert_eq!(uptime(90061.0), "1d 1h 1m");
    }
}
