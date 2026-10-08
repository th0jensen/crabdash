//! The full sampled process inventory has one virtualized, responsive browser.
use super::super::view::{bytes, card, heading};
use super::{Column, user_name};
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
use machines::resources::{ProcessUsage, ResourceUsage};
use std::rc::Rc;

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
    let tooltip = user_name(process).map_or_else(
        || format!("Name: {}", process.name),
        |user| format!("Name: {}\nUser: {user}", process.name),
    );
    clipped_text(process.name.clone())
        .id(SharedString::from(format!(
            "process-name-{}-{}",
            process.pid, process.start_id
        )))
        .flex_1()
        .min_w_0()
        .tooltip(move |_, cx| control_tooltip(tooltip.clone(), cx))
}
fn process_user(process: &ProcessUsage) -> Stateful<Div> {
    let user = user_name(process).unwrap_or("—").to_string();
    let tooltip = user_name(process)
        .map_or("User unavailable", |user| user)
        .to_string();
    clipped_text(user)
        .id(SharedString::from(format!(
            "process-user-{}-{}",
            process.pid, process.start_id
        )))
        .tooltip(move |_, cx| control_tooltip(tooltip.clone(), cx))
}
fn process_value(process: &ProcessUsage, field: &str, value: String) -> Stateful<Div> {
    let tooltip = value.clone();
    clipped_text(value)
        .id(SharedString::from(format!(
            "process-{field}-{}-{}",
            process.pid, process.start_id
        )))
        .tooltip(move |_, cx| control_tooltip(tooltip.clone(), cx))
}

#[derive(Clone, Copy)]
struct CompactColumns {
    pid: f32,
    memory: f32,
    gap: f32,
    inset: f32,
}

impl CompactColumns {
    fn for_width(width: f32) -> Self {
        if width < 320.0 {
            Self {
                pid: 44.0,
                memory: 80.0,
                gap: 4.0,
                inset: 6.0,
            }
        } else {
            Self {
                pid: 88.0,
                memory: 88.0,
                gap: 8.0,
                inset: 12.0,
            }
        }
    }
}

fn process_row(process: &ProcessUsage, compact: bool, columns: CompactColumns) -> Stateful<Div> {
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
                .px(px(columns.inset))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .gap(px(columns.gap))
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
                            fixed_column(columns.pid)
                                .flex_shrink()
                                .whitespace_nowrap()
                                .child(process_value(process, "pid", process.pid.to_string())),
                        )
                        .gap(px(columns.gap))
                        .child(
                            process_user(process)
                                .w(rems(64.0 / 16.0))
                                .flex_auto()
                                .min_w_0(),
                        )
                        .child(
                            fixed_column(columns.memory)
                                .flex_shrink()
                                .whitespace_nowrap()
                                .child(process_value(process, "memory", memory)),
                        ),
                )
        } else {
            table::table_row()
                .h_full()
                .child(
                    fixed_column(88.0)
                        .whitespace_nowrap()
                        .text_color(rgb(style::TEXT_MUTED))
                        .child(process_value(process, "pid", process.pid.to_string())),
                )
                .child(process_identity(process))
                .child(fixed_column(96.0).child(process_user(process)))
                .child(
                    fixed_column(64.0)
                        .whitespace_nowrap()
                        .child(clipped_text(cpu)),
                )
                .child(
                    fixed_column(88.0)
                        .whitespace_nowrap()
                        .child(process_value(process, "memory", memory)),
                )
        })
}
pub(crate) fn render(
    app: &Crabdash,
    usage: &Rc<ResourceUsage>,
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
    let query = state.search.query(cx);
    let prepared = state.prepare(app.selected_machine().uuid, usage, &query);
    let count = prepared.indices.len();
    let sampled = processes.len();
    let filtering = !query.trim().is_empty();
    let incomplete =
        usage.processes_truncated || usage.process_count.is_none_or(|total| total > sampled);
    let coverage = match usage.process_count {
        Some(total) if incomplete => format!("{sampled} of {total} sampled"),
        Some(_) => format!("{sampled} sampled"),
        None => format!("{sampled} sampled · total unavailable"),
    };
    let coverage = if usage.processes_truncated {
        format!("{coverage} · collection limit reached")
    } else if usage.process_count.is_some_and(|total| total > sampled) {
        format!("{coverage} · some unavailable")
    } else {
        coverage
    };
    let card = card()
        .child(heading(
            "Processes",
            if filtering {
                format!("{count} {}", if count == 1 { "match" } else { "matches" })
            } else {
                count.to_string()
            },
            "% of total CPU capacity".into(),
        ))
        .when(filtering || incomplete, |card| {
            card.child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_size(rems(style::META / 16.0))
                    .text_color(rgb(style::TEXT_MUTED))
                    .child(coverage),
            )
        })
        .child(state.search.render().w_full().min_w_0());
    if count == 0 {
        let fields = if processes.iter().any(|process| user_name(process).is_some()) {
            "name, PID, or user"
        } else {
            "name or PID"
        };
        let (title, description) = if processes.is_empty() {
            (
                "No processes sampled",
                if incomplete {
                    "No process details were available in the latest sample."
                } else {
                    "The latest sample did not report any processes."
                }
                .to_string(),
            )
        } else {
            (
                "No matching processes",
                if incomplete {
                    format!(
                        "Try another {fields}. The sample is incomplete; other processes may be unavailable."
                    )
                } else {
                    format!("Try another {fields}.")
                },
            )
        };
        return card.child(
            table::placeholder_card(title, &description)
                .flex_col()
                .items_start()
                .when(filtering, |this| {
                    this.child(
                        div().w_full().flex().justify_end().child(
                            surface_button(
                                "system-process-clear-filter",
                                Some(Icon::X),
                                Some("Clear filter"),
                            )
                            .on_click(cx.listener(|app, _, _, cx| {
                                if let Some(state) = app.system.processes.as_ref() {
                                    state.search.clear(cx);
                                }
                            })),
                        ),
                    )
                }),
        );
    }
    let compact = width < 600.0;
    let columns = CompactColumns::for_width(width);
    let heading_row = if compact {
        div()
            .flex()
            .flex_col()
            .child(
                table::table_heading()
                    .px(px(columns.inset))
                    .gap(px(columns.gap))
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
                    .px(px(columns.inset))
                    .gap(px(columns.gap))
                    .child(
                        fixed_column(columns.pid).flex_shrink().child(
                            process_heading(app, Column::Pid, "PID", cx)
                                .when(width < 320.0, |this| this.gap(px(2.0))),
                        ),
                    )
                    .child(
                        div()
                            .w(rems(64.0 / 16.0))
                            .flex_auto()
                            .min_w_0()
                            .child(process_heading(app, Column::User, "User", cx)),
                    )
                    .child(
                        fixed_column(columns.memory)
                            .flex_shrink()
                            .child(process_heading(
                                app,
                                Column::Memory,
                                if width < 320.0 { "RAM" } else { "Memory" },
                                cx,
                            )),
                    ),
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
            .child(fixed_column(96.0).child(process_heading(app, Column::User, "User", cx)))
            .child(fixed_column(64.0).child(process_heading(app, Column::Cpu, "CPU", cx)))
            .child(fixed_column(88.0).child(process_heading(app, Column::Memory, "Memory", cx)))
    };
    let scroll = state.scroll.clone();
    let list = uniform_list("system-process-list", count, move |range, _, _| {
        range
            .filter_map(|index| prepared.row(index))
            .map(|process| process_row(process, compact, columns))
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
        .child(list);
    card.child(table::table_card().min_w_0().child(heading_row).child(body))
}
