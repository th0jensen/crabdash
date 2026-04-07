use gpui::prelude::*;
use gpui::*;

use crate::{app::Crabdash, components::scroll_list};

use super::shared::{error_panel, placeholder_card};
use services::ServiceItem;

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

fn stats_chip(label: &str, value: String) -> Div {
    div()
        .h(px(34.0))
        .px(px(12.0))
        .py(px(7.0))
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x2F2F31))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(8.0))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x8E8E93))
                .child(label.to_string()),
        )
        .child(div().text_sm().text_color(white()).child(value))
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
    let services = &machine.services.systemd;

    if let Some(error) = machine.services.systemd_error.clone() {
        return error_panel("Unable to load services", error);
    }

    if services.is_empty() {
        return placeholder_card(
            "SYSTEM SERVICES",
            "No system services have been loaded for this machine yet.",
        );
    }

    let running_count = services
        .iter()
        .filter(|service| service.status.contains("0") || !service.status.contains("inactive"))
        .count();

    scroll_list::render(
        "services-scroll",
        &app.services_scroll_handle,
        Some(
            div()
                .flex()
                .gap(px(8.0))
                .child(stats_chip("Total", services.len().to_string()))
                .child(stats_chip("Running", running_count.to_string()))
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
            .children(services.iter().map(system_service_row)),
        cx,
    )
}
