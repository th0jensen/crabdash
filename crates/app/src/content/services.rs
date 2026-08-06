use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use services::{ServiceAction, Services};
use utils::service_item::ServiceItem;

use crate::{
    app::Crabdash,
    components::{common::lucide_icon, scroll_list},
};

use super::shared::{error_panel, placeholder_card};
use services::ServiceFilter;

fn status_badge(service: &ServiceItem, pending_action: Option<ServiceAction>) -> Div {
    let label = pending_action
        .map(|action| action.pending_label())
        .unwrap_or_else(|| service.status_label());
    let is_running = pending_action.is_none() && service.is_running();
    let is_pending = pending_action.is_some();
    let is_failed = label == "Failed";
    let status_fg = if is_running {
        rgb(0x30D158)
    } else if is_pending {
        rgb(0xFFD60A)
    } else if is_failed {
        rgb(0xFF453A)
    } else {
        rgb(0x8E8E93)
    };

    div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(5.0))
        .text_xs()
        .text_color(status_fg)
        .child(div().size(px(6.0)).rounded_full().bg(status_fg))
        .child(label)
}

fn service_metadata(service: &ServiceItem) -> String {
    let mut metadata = Vec::new();

    if let Some(load_state) = service.load_state.as_ref() {
        metadata.push(load_state.clone());
    }

    let state = service
        .sub_state
        .as_ref()
        .map(|sub_state| format!("{} ({sub_state})", service.status))
        .unwrap_or_else(|| service.status.clone());
    if !state.trim().is_empty() {
        metadata.push(state);
    }

    if let Some(unit_file_state) = service.unit_file_state.as_ref() {
        metadata.push(unit_file_state.clone());
    }

    if !matches!(service.id.trim(), "" | "-" | "0") {
        metadata.push(format!("PID {}", service.id));
    }

    metadata.join("  ·  ")
}

fn stats_chip(
    id: impl Into<ElementId>,
    label: &str,
    value: String,
    active: bool,
    filter: ServiceFilter,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let bg = if active { rgb(0x2A2A2A) } else { rgb(0x181818) };
    let label_color = if active { rgb(0xAEAEB2) } else { rgb(0x8E8E93) };
    let value_color = if active { rgb(0xFFFFFF) } else { rgb(0xAEAEB2) };

    div()
        .id(id)
        .h(px(32.0))
        .px(px(11.0))
        .bg(bg)
        .flex()
        .items_center()
        .gap(px(7.0))
        .rounded(px(3.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2A2C)))
        .child(
            div()
                .text_xs()
                .text_color(label_color)
                .child(label.to_string()),
        )
        .child(div().text_sm().text_color(value_color).child(value))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.service_filter = filter;
            cx.notify();
        }))
}

fn summary_metric(label: &str, value: usize, color: Rgba) -> Div {
    div()
        .h(px(32.0))
        .px(px(11.0))
        .bg(rgb(0x181818))
        .rounded(px(3.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .text_xs()
        .text_color(rgb(0x8E8E93))
        .child(label.to_string())
        .child(div().text_color(color).child(value.to_string()))
}

fn table_header() -> Div {
    div()
        .h(px(34.0))
        .px(px(12.0))
        .bg(rgb(0x1B1B1B))
        .border_b_1()
        .border_color(rgb(0x2B2B2B))
        .flex()
        .items_center()
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(0x737373))
        .child(div().w(px(220.0)).child("UNIT"))
        .child(div().flex_1().child("DESCRIPTION"))
        .child(div().w(px(260.0)).child("STATE"))
        .child(div().w(px(182.0)).text_right().child("ACTIONS  ·  STATUS"))
}

fn service_action_button(
    cx: &mut Context<Crabdash>,
    service: &ServiceItem,
    action: ServiceAction,
    disabled: bool,
) -> impl IntoElement {
    let bg = rgba(0x00000000);
    let disabled_bg = rgba(0x00000000);
    let disabled_fg = rgb(0x606060);
    let hover_bg = rgb(0x303030);

    let name = service.name.clone();
    let button_id = SharedString::from(format!("{}-service-{name}", action.command()));

    let button = div()
        .id(button_id)
        .h(px(30.0))
        .w(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(if disabled { disabled_bg } else { bg })
        .rounded(px(3.0))
        .text_color(if disabled { disabled_fg } else { rgb(0xFFFFFF) })
        .child(lucide_icon(action.icon(), 14.0));

    if disabled {
        button.cursor_default()
    } else {
        button
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .on_click(cx.listener(move |this, _, _, cx| {
                let machine_index = this.selected_machine;
                let mut machine = this.selected_machine().clone();

                if let Some(machine) = this.machine_store.machines.get_mut(machine_index) {
                    this.pending_service_actions.insert(name.clone(), action);
                    if let Some(service) = machine.services.systemd.iter_mut().find(|service| service.name == name) {
                        service.error = None;
                    }
                }

                cx.notify();

                let spawn_name = name.clone();
                let update_name = name.clone();

                cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        let result = cx
                            .background_spawn({
                                let service_name = spawn_name.clone();
                                async move {
                                    machine.service_action(&service_name, action).await?;
                                    machine.list_services().await
                                }
                            })
                            .await;

                        this.update(&mut cx, move |this, cx| {
                            this.pending_service_actions.remove(&update_name);
                            match result {
                                Ok(services) => {
                                    if let Some(machine) =
                                        this.machine_store.machines.get_mut(machine_index)
                                    {
                                        machine.services.systemd = services;
                                        machine.services.systemd_error = None;
                                    }
                                    this.clear_status_message();
                                }
                                Err(err) => {
                                    let message =
                                        format!("Failed to {} {update_name}: {err}", action.command());
                                    tracing::warn!(error = %err, action = action.command(), service = %update_name, "Service action failed");
                                    this.set_status_error(message.clone());
                                    if let Some(machine) =
                                        this.machine_store.machines.get_mut(machine_index)
                                    {
                                        if let Some(service) = machine
                                            .services
                                            .systemd
                                            .iter_mut()
                                            .find(|service| service.name == update_name)
                                        {
                                            service.error = Some(message);
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        })
                        .ok();
                    }
                })
                .detach();
            }))
    }
}

fn service_logs_button(
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
    service: &ServiceItem,
) -> impl IntoElement {
    let service_name = service.name.clone();
    let log_key = (app.selected_machine().uuid, service_name.clone());
    let logs_open = app.logs_open_services.contains(&log_key);
    let bg = if logs_open {
        rgb(0x303030)
    } else {
        rgba(0x00000000)
    };

    div()
        .id(SharedString::from(format!("logs-service-{service_name}")))
        .h(px(30.0))
        .w(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .rounded(px(3.0))
        .text_color(rgb(0xFFFFFF))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(0x2F2F31)))
        .child(lucide_icon(Icon::ChartNoAxesGantt, 14.0))
        .on_click(cx.listener(move |this, _, _, cx| {
            if this.logs_open_services.contains(&log_key) {
                this.logs_open_services.remove(&log_key);
                cx.notify();
                return;
            }

            this.logs_open_services.insert(log_key.clone());

            {
                let state = match super::terminal::TerminalState::new_log(500) {
                    Ok(state) => state,
                    Err(err) => {
                        this.set_status_error(format!("Failed to init terminal: {err}"));
                        cx.notify();
                        return;
                    }
                };
                this.expanded_service_logs.insert(log_key.clone(), state);

                let mut machine = this.selected_machine().clone();
                let fetch_service_name = service_name.clone();
                let fetch_key = log_key.clone();
                cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        let result = cx
                            .background_spawn({
                                let service_name = fetch_service_name.clone();
                                async move { machine.service_logs(&service_name).await }
                            })
                            .await;
                        this.update(&mut cx, move |this, cx| {
                            if let Some(state) = this.expanded_service_logs.get_mut(&fetch_key) {
                                match result {
                                    Ok(logs) => state.feed(logs),
                                    Err(err) => state.feed_string(format!("Error: {err}")),
                                }
                            }
                            cx.notify();
                        })
                        .ok();
                    }
                })
                .detach();
            }

            cx.notify();
        }))
}

fn system_service_row(app: &Crabdash, cx: &mut Context<Crabdash>, service: &ServiceItem) -> Div {
    let service_name = service.name.clone();
    let pending_action = app.pending_service_actions.get(&service_name).copied();
    let actions_disabled = pending_action.is_some();
    let log_key = (app.selected_machine().uuid, service_name.clone());
    let logs_open = app.logs_open_services.contains(&log_key);
    let state = app.expanded_service_logs.get(&log_key);
    let scroll_handle = state
        .map(|state| state.scroll_handle.clone())
        .unwrap_or_default();
    let wheel_handle = scroll_handle.clone();
    let log_height = state
        .map(|state| super::terminal::viewport_height(&state.rendered))
        .unwrap_or_else(super::terminal::minimum_viewport_height);

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .px(px(12.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(rgb(0x2B2B2B))
                .flex()
                .justify_between()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .w(px(220.0))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(px(13.0))
                                .text_color(rgb(0xD4D4D4))
                                .child(service.name.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(px(13.0))
                                .text_color(rgb(0xA0A0A0))
                                .child(service.description.clone().unwrap_or_default()),
                        )
                        .child(
                            div()
                                .w(px(260.0))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(px(11.0))
                                .text_color(rgb(0x777777))
                                .child(service_metadata(service)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .child(service_logs_button(app, cx, service))
                        .child(if !service.is_running() {
                            service_action_button(
                                cx,
                                service,
                                ServiceAction::Start,
                                actions_disabled,
                            )
                        } else {
                            service_action_button(
                                cx,
                                service,
                                ServiceAction::Stop,
                                actions_disabled,
                            )
                        })
                        .child(service_action_button(
                            cx,
                            service,
                            ServiceAction::Restart,
                            actions_disabled,
                        ))
                        .child(
                            status_badge(service, pending_action)
                                .w(px(72.0))
                                .text_center(),
                        ),
                ),
        )
        .when(logs_open, |this| {
            let logs_content = match state {
                Some(state) if !state.loaded => div()
                    .w_full()
                    .text_xs()
                    .text_color(rgb(0x8E8E93))
                    .child("Loading logs...")
                    .into_any_element(),
                Some(state) if state.rendered.is_empty() => div()
                    .w_full()
                    .text_xs()
                    .text_color(rgb(0x8E8E93))
                    .child("No logs available.")
                    .into_any_element(),
                Some(state) => div()
                    .w_full()
                    .child(super::terminal::render_view(&state.rendered))
                    .into_any_element(),
                None => div()
                    .w_full()
                    .text_xs()
                    .text_color(rgb(0x8E8E93))
                    .child("Loading logs...")
                    .into_any_element(),
            };

            this.child(
                div()
                    .w_full()
                    .px(px(10.0))
                    .pb(px(10.0))
                    .bg(rgb(0x111111))
                    .child(
                        div()
                            .pb(px(8.0))
                            .text_xs()
                            .text_color(rgb(0x8E8E93))
                            .child("Recent logs"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "service-logs-scroll-{service_name}"
                            )))
                            .w_full()
                            .h(log_height)
                            .track_scroll(&scroll_handle)
                            .overflow_scroll()
                            .on_scroll_wheel(cx.listener(
                                move |_, event: &ScrollWheelEvent, window, cx| {
                                    let delta = event.delta.pixel_delta(window.line_height());
                                    let current = wheel_handle.offset();
                                    let max = wheel_handle.max_offset();
                                    let next_x = (current.x + delta.x).max(-max.width).min(px(0.0));
                                    let next_y =
                                        (current.y + delta.y).max(-max.height).min(px(0.0));
                                    wheel_handle.set_offset(point(next_x, next_y));
                                    cx.notify();
                                    cx.stop_propagation();
                                },
                            ))
                            .child(logs_content),
                    ),
            )
        })
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let machine = app.selected_machine();
    let services = machine.services.systemd.clone();

    if let Some(error) = machine.services.systemd_error.clone() {
        return error_panel("Unable to load services", error);
    }

    if services.is_empty() {
        return placeholder_card(
            "SYSTEM SERVICES",
            "No system services have been loaded for this machine yet.",
        );
    }

    let total_count = services.len();
    let running_count = services.iter().filter(|item| item.is_running()).count();
    let failed_count = services
        .iter()
        .filter(|item| item.status_label() == "Failed")
        .count();
    let visible_services: Vec<ServiceItem> = match app.service_filter {
        ServiceFilter::Total => services,
        ServiceFilter::Running => services
            .clone()
            .into_iter()
            .filter(|item| item.is_running())
            .collect(),
    };

    scroll_list::render(
        "services-scroll",
        &app.services_scroll_handle,
        Some(
            div()
                .flex()
                .gap(px(8.0))
                .child(stats_chip(
                    "service-filter-total",
                    "Total",
                    total_count.to_string(),
                    app.service_filter == ServiceFilter::Total,
                    ServiceFilter::Total,
                    cx,
                ))
                .child(stats_chip(
                    "service-filter-active",
                    "Active",
                    running_count.to_string(),
                    app.service_filter == ServiceFilter::Running,
                    ServiceFilter::Running,
                    cx,
                ))
                .child(summary_metric("Failed", failed_count, rgb(0xF14C4C)))
                .into_any_element(),
        ),
        div()
            .flex()
            .flex_col()
            .gap(px(0.0))
            .overflow_hidden()
            .bg(rgb(0x181818))
            .border_1()
            .border_color(rgb(0x2B2B2B))
            .child(table_header())
            .children(
                visible_services
                    .iter()
                    .map(|service| system_service_row(app, cx, service)),
            ),
        cx,
    )
}
