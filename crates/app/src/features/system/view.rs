use super::MachineState;
use crate::{
    app::Crabdash,
    components::{
        common::{clipped_text, control_tooltip, surface_button},
        style,
    },
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;

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

fn chart(values: Vec<Option<f64>>, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            if values.len() < 2 {
                return;
            }
            let mut path = PathBuilder::stroke(px(1.5));
            let mut connected = false;
            for (index, value) in values.iter().enumerate() {
                let Some(value) = value.filter(|value| value.is_finite()) else {
                    connected = false;
                    continue;
                };
                let position = point(
                    bounds.left() + bounds.size.width * (index as f32 / (values.len() - 1) as f32),
                    bounds.bottom() - bounds.size.height * (value.clamp(0.0, 100.0) as f32 / 100.0),
                );
                if connected {
                    path.line_to(position);
                } else {
                    path.move_to(position);
                    connected = true;
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(color));
            }
        },
    )
    .w_full()
    .h(rems(64.0 / 16.0))
}

fn card() -> Div {
    div()
        .w_full()
        .min_w_0()
        .p(rems(14.0 / 16.0))
        .bg(rgb(style::SURFACE))
        .border_1()
        .border_color(rgb(style::BORDER))
        .rounded(px(style::CARD_RADIUS))
        .flex()
        .flex_col()
        .gap(rems(10.0 / 16.0))
}

fn metric_heading(name: &str, value: String, detail: String) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_start()
        .justify_between()
        .gap(rems(12.0 / 16.0))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name.to_string()),
                )
                .child(
                    clipped_text(detail)
                        .text_size(rems(style::META / 16.0))
                        .text_color(rgb(style::TEXT_MUTED)),
                ),
        )
        .child(div().flex_none().text_size(rems(20.0 / 16.0)).child(value))
}

fn fact(label: &str, value: String) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .gap(rems(12.0 / 16.0))
        .child(
            div()
                .w(rems(88.0 / 16.0))
                .flex_none()
                .whitespace_nowrap()
                .text_color(rgb(style::TEXT_MUTED))
                .child(label.to_string()),
        )
        .child(clipped_text(value).flex_1())
}

fn resource_cards(state: &MachineState) -> Div {
    let Some(usage) = state.usage.as_ref() else {
        return div();
    };
    let cpu_history: Vec<_> = state.history.iter().map(|point| point.cpu).collect();
    let memory_history: Vec<_> = state
        .history
        .iter()
        .map(|point| Some(point.memory))
        .collect();
    let cores = usage
        .cores
        .iter()
        .map(|core| {
            div()
                .w(rems(108.0 / 16.0))
                .max_w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .justify_between()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_color(rgb(style::TEXT_MUTED))
                                .child(format!("CPU {}", core.name)),
                        )
                        .child(percent(core.percent)),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(3.0))
                        .rounded(px(2.0))
                        .bg(rgb(style::BORDER))
                        .child(
                            div()
                                .h_full()
                                .w(relative(core.percent.unwrap_or(0.0) as f32 / 100.0))
                                .rounded(px(2.0))
                                .bg(rgb(style::TAB_INDICATOR)),
                        ),
                )
        })
        .collect::<Vec<_>>();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(rems(12.0 / 16.0))
        .child(
            card()
                .child(metric_heading(
                    "CPU",
                    percent(usage.cpu_percent),
                    format!("{} logical processors", usage.logical_cpus),
                ))
                .child(chart(cpu_history, style::SUCCESS))
                .when(!cores.is_empty(), |card| {
                    card.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(rems(12.0 / 16.0))
                            .text_size(rems(style::META / 16.0))
                            .children(cores),
                    )
                }),
        )
        .child(
            card()
                .child(metric_heading(
                    "Memory",
                    percent(Some(usage.memory.used_percent())),
                    format!(
                        "{} / {} used",
                        bytes(usage.memory.used_bytes()),
                        bytes(usage.memory.total_bytes)
                    ),
                ))
                .child(chart(memory_history, style::TAB_INDICATOR))
                .child(fact(
                    if usage.memory.estimated {
                        "Available ≈"
                    } else {
                        "Available"
                    },
                    bytes(usage.memory.available_bytes),
                ))
                .when_some(usage.swap, |card, swap| {
                    card.child(fact(
                        "Swap",
                        format!(
                            "{} / {} used",
                            bytes(swap.used_bytes()),
                            bytes(swap.total_bytes)
                        ),
                    ))
                }),
        )
        .child(
            card()
                .child(fact("Uptime", uptime(usage.uptime_seconds)))
                .when_some(usage.load_average, |card, load| {
                    card.child(fact(
                        "Load",
                        format!("{:.2}  ·  {:.2}  ·  {:.2}", load[0], load[1], load[2]),
                    ))
                }),
        )
}

pub(crate) fn render(app: &Crabdash, _: &Window, cx: &mut Context<Crabdash>) -> Stateful<Div> {
    let machine = app.selected_machine();
    let uuid = machine.uuid;
    let state = app.system.machines.get(&uuid);
    let info = &machine.system_info;
    let distribution = info.distribution.as_ref().map_or_else(
        || info.platform_label().to_string(),
        |distribution| distribution.pretty_name.clone(),
    );
    let label = state.and_then(|state| state.updated).map_or_else(
        || "Starting…".to_string(),
        |updated| {
            if updated.elapsed().as_secs() > 6 {
                "Waiting for a sample…".into()
            } else {
                "Live · 2s".into()
            }
        },
    );
    div()
        .id("system-resources")
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(rems(12.0 / 16.0))
        .text_color(rgb(style::TEXT_PRIMARY))
        .text_size(rems(style::TEXT / 16.0))
        .child(
            div()
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
                            control_tooltip(
                                "Refresh resources · live sampling runs every 2 seconds",
                                cx,
                            )
                        })
                        .on_click(cx.listener(|app, _, _, cx| app.refresh_system_resources(cx))),
                ),
        )
        .when_some(
            state.and_then(|state| state.error.as_ref()),
            |root, error| root.child(card().text_color(rgb(style::DANGER)).child(error.clone())),
        )
        .when_some(state, |root, state| root.child(resource_cards(state)))
        .when(state.is_none_or(|state| state.usage.is_none()), |root| {
            root.child(
                card()
                    .text_color(rgb(style::TEXT_MUTED))
                    .child("Collecting resource samples…"),
            )
        })
        .child(
            card()
                .child(fact("Machine", info.machine_name.clone()))
                .child(fact("Platform", distribution))
                .child(fact("Kernel", info.os_version.clone()))
                .child(fact("Architecture", info.arch.clone())),
        )
        .child(
            div()
                .text_size(rems(style::META / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child("Recent samples · CPU needs two samples · memory excludes available cache"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::test;
    #[test]
    fn readable_units_and_unknown_cpu_are_explicit() {
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
        assert_eq!(percent(None), "Sampling…");
        assert_eq!(percent(Some(f64::NAN)), "Sampling…");
        assert_eq!(uptime(90061.0), "1d 1h 1m");
    }
}
