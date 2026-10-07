//! Responsive resource mosaic: the pane's width decides the arrangement.
use super::{MachineState, processes::Column, sum_rates};
use crate::{
    app::Crabdash,
    components::{
        common::{clipped_text, control_tooltip, surface_button},
        style,
        table::{self, fixed_column, sort_heading},
    },
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use machines::resources::{GpuSample, ProcessUsage, ResourceUsage};

fn percent(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(|| "Sampling…".into(), |value| format!("{value:.1}%"))
}
fn bytes(value: u64) -> String {
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
fn chart(series: Vec<(Vec<Option<f64>>, u32)>, fixed_max: Option<f64>) -> impl IntoElement {
    let maximum = fixed_max.unwrap_or_else(|| {
        series
            .iter()
            .flat_map(|(values, _)| values)
            .filter_map(|value| *value)
            .filter(|value| value.is_finite())
            .fold(1.0_f64, f64::max)
    });
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for (values, color) in &series {
                if values.len() < 2 {
                    continue;
                }
                let mut path = PathBuilder::stroke(px(1.5));
                let mut connected = false;
                for (index, value) in values.iter().enumerate() {
                    let Some(value) = value.filter(|value| value.is_finite()) else {
                        connected = false;
                        continue;
                    };
                    let position = point(
                        bounds.left()
                            + bounds.size.width * (index as f32 / (values.len() - 1) as f32),
                        bounds.bottom()
                            - bounds.size.height * (value.clamp(0.0, maximum) / maximum) as f32,
                    );
                    if connected {
                        path.line_to(position);
                    } else {
                        path.move_to(position);
                        connected = true;
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(*color));
                }
            }
        },
    )
    .w_full()
    .h(rems(56.0 / 16.0))
    .flex_none()
}
fn card() -> Div {
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
fn heading(name: &str, value: String, detail: String) -> Div {
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
        .child(chart(
            vec![(
                state.history.iter().map(|point| point.cpu).collect(),
                style::TEXT_PRIMARY,
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
        .child(chart(
            vec![(
                state
                    .history
                    .iter()
                    .map(|point| Some(point.memory))
                    .collect(),
                style::TEXT_MUTED,
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
        .child(chart(
            vec![
                (
                    state.history.iter().map(|point| point.network_rx).collect(),
                    style::TEXT_PRIMARY,
                ),
                (
                    state.history.iter().map(|point| point.network_tx).collect(),
                    style::TEXT_MUTED,
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
        .child(chart(
            vec![
                (
                    state.history.iter().map(|point| point.disk_read).collect(),
                    style::TEXT_PRIMARY,
                ),
                (
                    state.history.iter().map(|point| point.disk_write).collect(),
                    style::TEXT_MUTED,
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
                    .any(|value| value.is_some_and(|value| value.is_finite()))
            }),
            |this, history| {
                this.child(chart(
                    vec![(history.iter().copied().collect(), style::TEXT_PRIMARY)],
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
fn process_heading(
    app: &Crabdash,
    column: Column,
    label: &str,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let direction = app
        .system
        .processes
        .as_ref()
        .and_then(|state| state.sort.indicator(column));
    sort_heading(
        SharedString::from(format!("process-sort-{column:?}")),
        label,
        direction,
    )
    .on_click(cx.listener(move |app, _, _, cx| {
        if let Some(state) = app.system.processes.as_mut() {
            state.sort.select(column);
            state.scroll.set_offset(point(px(0.0), px(0.0)));
            cx.notify();
        }
    }))
}
fn process_identity(process: &ProcessUsage) -> Stateful<Div> {
    clipped_text(process.name.clone())
        .id(SharedString::from(format!(
            "process-name-{}-{}",
            process.pid, process.start_id
        )))
        .flex_1()
        .min_w_0()
        .when_some(process.user.as_ref(), |this, user| {
            this.tooltip({
                let user = user.clone();
                move |_, cx| control_tooltip(format!("User: {user}"), cx)
            })
        })
}
fn process_row(process: &ProcessUsage, compact: bool) -> Stateful<Div> {
    let cpu = process
        .cpu_percent
        .filter(|value| value.is_finite())
        .map_or_else(|| "—".into(), |value| format!("{value:.1}%"));
    let memory = process.memory_bytes.map_or_else(|| "—".into(), bytes);
    div()
        .id(SharedString::from(format!(
            "process-{}-{}",
            process.pid, process.start_id
        )))
        .w_full()
        .min_w_0()
        .when(compact, |this| this.py(px(7.0)))
        .when(!compact, |this| this.h(rems(style::BAR / 16.0)))
        .border_b_1()
        .border_color(rgb(style::BORDER))
        .hover(|this| this.bg(rgb(style::SURFACE_HOVER)))
        .child(if compact {
            div()
                .w_full()
                .min_w_0()
                .px(px(12.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .gap(px(8.0))
                        .child(process_identity(process))
                        .child(
                            fixed_column(64.0)
                                .whitespace_nowrap()
                                .child(clipped_text(cpu)),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .justify_between()
                        .text_size(rems(style::META / 16.0))
                        .text_color(rgb(style::TEXT_MUTED))
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .child(format!("PID {}", process.pid)),
                        )
                        .child(
                            fixed_column(88.0)
                                .whitespace_nowrap()
                                .child(clipped_text(memory)),
                        ),
                )
        } else {
            table::table_row()
                .h_full()
                .child(
                    fixed_column(88.0)
                        .whitespace_nowrap()
                        .text_color(rgb(style::TEXT_MUTED))
                        .child(clipped_text(process.pid.to_string())),
                )
                .child(process_identity(process))
                .child(
                    fixed_column(64.0)
                        .whitespace_nowrap()
                        .child(clipped_text(cpu)),
                )
                .child(
                    fixed_column(88.0)
                        .whitespace_nowrap()
                        .child(clipped_text(memory)),
                )
        })
}
fn process_card(
    app: &Crabdash,
    usage: &ResourceUsage,
    width: f32,
    cx: &mut Context<Crabdash>,
) -> Div {
    let Some(processes) = &usage.processes else {
        return card().child(heading(
            "Processes",
            "Unavailable".into(),
            "Process sampling is not available on this machine.".into(),
        ));
    };
    let Some(state) = app.system.processes.as_ref() else {
        return card().child("Preparing process list…");
    };
    let rows = state.visible(processes, &state.search.query(cx));
    let compact = width < 460.0;
    let heading_row = if compact {
        div()
            .flex()
            .flex_col()
            .child(
                table::table_heading()
                    .child(div().flex_1().min_w_0().child(process_heading(
                        app,
                        Column::Name,
                        "Name",
                        cx,
                    )))
                    .child(fixed_column(64.0).child(process_heading(app, Column::Cpu, "CPU", cx))),
            )
            .child(
                table::table_heading()
                    .child(div().flex_1().min_w_0().child(process_heading(
                        app,
                        Column::Pid,
                        "PID",
                        cx,
                    )))
                    .child(fixed_column(88.0).child(process_heading(
                        app,
                        Column::Memory,
                        "Memory",
                        cx,
                    ))),
            )
    } else {
        table::table_heading()
            .child(fixed_column(88.0).child(process_heading(app, Column::Pid, "PID", cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(process_heading(app, Column::Name, "Name", cx)),
            )
            .child(fixed_column(64.0).child(process_heading(app, Column::Cpu, "CPU", cx)))
            .child(fixed_column(88.0).child(process_heading(app, Column::Memory, "Memory", cx)))
    };
    let count = rows.len();
    let snapshot: Vec<ProcessUsage> = rows.into_iter().cloned().collect();
    let scroll = state.scroll.clone();
    let list = uniform_list("system-process-list", count, move |range, _, _| {
        range
            .filter_map(|index| snapshot.get(index))
            .map(|process| process_row(process, compact))
            .collect::<Vec<_>>()
    })
    .size_full()
    .track_scroll(state.list_scroll.clone())
    .on_scroll_wheel(cx.listener(move |_, event: &ScrollWheelEvent, window, cx| {
        let delta = event.delta.pixel_delta(window.line_height());
        let offset = scroll.offset();
        // GPUI's native handler already changed this list's base offset.
        let movement = if delta.y.is_zero() { delta.x } else { delta.y };
        let lower = -scroll.max_offset().height;
        let previous_y = (offset.y - movement).max(lower).min(px(0.0));
        let next_y = offset.y.max(lower).min(px(0.0));
        scroll.set_offset(point(offset.x, next_y));
        if next_y != previous_y {
            cx.notify();
            cx.stop_propagation();
        }
    }));
    let body = div()
        .h(rems(264.0 / 16.0))
        .w_full()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .child(if count > 0 {
            list.into_any_element()
        } else {
            div()
                .p(px(12.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child(if processes.is_empty() {
                    "No processes in the latest sample."
                } else {
                    "No matching processes."
                })
                .into_any_element()
        });
    card()
        .child(heading(
            "Processes",
            format!(
                "{count} / {}",
                usage.process_count.unwrap_or(processes.len())
            ),
            if usage.processes_truncated
                || usage
                    .process_count
                    .is_some_and(|total| processes.len() < total)
            {
                format!("Top {} sampled · % of total CPU capacity", processes.len())
            } else {
                "% of total CPU capacity".into()
            },
        ))
        .child(state.search.render().w_full().min_w_0())
        .child(table::table_card().min_w_0().child(heading_row).child(body))
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
    let label: String = state.and_then(|state| state.updated).map_or_else(
        || "Starting…".into(),
        |updated| {
            if updated.elapsed().as_secs() > 6 {
                "Waiting for a sample…".into()
            } else {
                "Live · 2s".into()
            }
        },
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
            .child(fact("Name", info.machine_name.clone()))
            .child(fact("Platform", distribution))
            .child(fact("Kernel", info.os_version.clone()))
            .child(fact("Architecture", info.arch.clone()));
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
        mosaic = mosaic.child(process_card(app, usage, width, cx).col_span_full());
    } else {
        mosaic = mosaic.child(
            card()
                .col_span_full()
                .text_color(rgb(style::TEXT_MUTED))
                .child("Collecting resource samples…"),
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
                .text_size(rems(style::META / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child(label),
        )
        .child(
            surface_button("system-refresh", Some(Icon::RefreshCw), None)
                .tooltip(|_, cx| {
                    control_tooltip("Refresh resources · live sampling runs every 2 seconds", cx)
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
