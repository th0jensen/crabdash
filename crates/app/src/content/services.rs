use gpui::prelude::*;
use gpui::*;
use utils::service_item::ServiceItem;

use crate::{app::Crabdash, components::scroll_list};

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

fn system_service_row(service: &ServiceItem) -> Div {
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
        .child(status_badge(&service.status))
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
            .children(visible_services.iter().map(system_service_row)),
        cx,
    )
}
