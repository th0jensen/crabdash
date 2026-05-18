use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use services::Services;
use utils::service_item::ServiceItem;

use crate::{
    app::Crabdash,
    components::{common::lucide_icon, scroll_list},
};

use super::shared::{error_panel, placeholder_card};
use services::ServiceFilter;

fn status_badge(status: &str) -> Div {
    let normalized = status.to_ascii_lowercase();
    let is_running = normalized.contains("0") || !normalized.contains("inactive");
    let status_bg = if is_running {
        rgb(0x193D2A)
    } else {
        rgb(0x47232B)
    };
    let status_fg = if is_running {
        rgb(0x30D158)
    } else {
        rgb(0xFF453A)
    };

    div()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(999.0))
        .bg(status_bg)
        .text_xs()
        .text_color(status_fg)
        .child(status.to_string())
}

fn stats_chip(
    id: impl Into<ElementId>,
    label: &str,
    value: String,
    active: bool,
    filter: ServiceFilter,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let bg = if active { rgb(0x2C2C2E) } else { rgb(0x1C1C1E) };
    let label_color = if active { rgb(0xAEAEB2) } else { rgb(0x8E8E93) };
    let value_color = if active { rgb(0xFFFFFF) } else { rgb(0xAEAEB2) };

    div()
        .id(id)
        .h(px(34.0))
        .px(px(12.0))
        .py(px(7.0))
        .bg(bg)
        .border_1()
        .border_color(rgb(0x2F2F31))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(8.0))
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

fn service_logs_button(
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
    service: &ServiceItem,
) -> impl IntoElement {
    let service_name = service.name.clone();
    let logs_open = app.logs_open_services.contains(&service_name);
    let bg = if logs_open {
        rgb(0x1F3656)
    } else {
        rgb(0x242426)
    };
    let border_color = if logs_open {
        rgb(0x0A84FF)
    } else {
        rgb(0x2F2F31)
    };

    div()
        .id(SharedString::from(format!("logs-service-{service_name}")))
        .h(px(34.0))
        .w(px(34.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .border_1()
        .border_color(border_color)
        .rounded(px(8.0))
        .text_color(rgb(0xFFFFFF))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(0x2F2F31)))
        .child(lucide_icon(Icon::ChartNoAxesGantt, 14.0))
        .on_click(cx.listener(move |this, _, _, cx| {
            if this.logs_open_services.contains(&service_name) {
                this.logs_open_services.remove(&service_name);
                cx.notify();
                return;
            }

            this.logs_open_services.insert(service_name.clone());

            if !this.expanded_service_logs.contains_key(&service_name) {
                let state = match super::terminal_logs::TerminalLogState::new(500) {
                    Ok(state) => state,
                    Err(err) => {
                        this.set_status_error(format!("Failed to init terminal: {err}"));
                        cx.notify();
                        return;
                    }
                };
                this.expanded_service_logs
                    .insert(service_name.clone(), state);

                let mut machine = this.selected_machine().clone();
                let fetch_service_name = service_name.clone();
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
                            if let Some(state) =
                                this.expanded_service_logs.get_mut(&fetch_service_name)
                            {
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
    let logs_open = app.logs_open_services.contains(&service_name);
    let state = app.expanded_service_logs.get(&service_name);
    let scroll_handle = state
        .map(|state| state.scroll_handle.clone())
        .unwrap_or_default();
    let wheel_handle = scroll_handle.clone();
    let log_height = state
        .map(|state| super::terminal_logs::viewport_height(&state.rendered))
        .unwrap_or_else(super::terminal_logs::minimum_viewport_height);

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .px(px(14.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(rgb(0x2F2F31))
                .flex()
                .justify_between()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_sm()
                                .text_color(white())
                                .child(service.name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x8E8E93))
                                .child(format!("{}", service.id)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(service_logs_button(app, cx, service))
                        .child(status_badge(&service.status)),
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
                    .child(super::terminal_logs::render_view(&state.rendered))
                    .into_any_element(),
                None => div()
                    .w_full()
                    .text_xs()
                    .text_color(rgb(0x8E8E93))
                    .child("Loading logs...")
                    .into_any_element(),
            };

            this.child(
                div().w_full().px(px(14.0)).pb(px(12.0)).child(
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
                                let next_y = (current.y + delta.y).max(-max.height).min(px(0.0));
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
                .into_any_element(),
        ),
        div()
            .flex()
            .flex_col()
            .gap(px(0.0))
            .bg(rgb(0x2C2C2E))
            .border_1()
            .border_color(rgb(0x2F2F31))
            .rounded(px(8.0))
            .children(
                visible_services
                    .iter()
                    .map(|service| system_service_row(app, cx, service)),
            ),
        cx,
    )
}
