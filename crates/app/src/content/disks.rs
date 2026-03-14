use gpui::prelude::*;
use gpui::*;

use crate::app::Crabdash;

use super::shared::{error_panel, placeholder_card};
use services::ServiceItem;

fn status_badge(status: &str) -> Div {
    let normalized = status.to_ascii_lowercase();
    let is_healthy = normalized.contains("mounted") || normalized.contains("healthy");
    let status_bg = if is_healthy {
        rgb(0x193D2A)
    } else {
        rgb(0x47232B)
    };
    let status_fg = if is_healthy {
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
        .border_color(rgb(0x3A3A3C))
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

fn disk_row(service: &ServiceItem) -> Div {
    div()
        .w_full()
        .px(px(14.0))
        .py(px(12.0))
        .border_b_1()
        .border_color(rgb(0x3A3A3C))
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
                .child(div().text_xs().text_color(rgb(0x8E8E93)).child(format!(
                    "{} • {}",
                    service.kind.label(),
                    service.id
                ))),
        )
        .child(status_badge(&service.status))
}

pub fn render(app: &Crabdash, _cx: &mut Context<Crabdash>) -> Div {
    let machine = app.selected_machine();
    let services = &machine.services.disks;

    if let Some(error) = machine.services.disks_error.clone() {
        return error_panel("Unable to load disks", error);
    }

    if services.is_empty() {
        return placeholder_card(
            "DISKS",
            "No disk items have been loaded for this machine yet.",
        );
    }

    let healthy_count = services
        .iter()
        .filter(|service| service.status.contains("mounted") || service.status.contains("healthy"))
        .count();

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(stats_chip("Total", services.len().to_string()))
                .child(stats_chip("Healthy", healthy_count.to_string())),
        )
        .child(
            div()
                .id("disks-scroll")
                .bg(rgb(0x2C2C2E))
                .border_1()
                .border_color(rgb(0x3A3A3C))
                .overflow_y_scroll()
                .child(
                    div()
                        .px(px(14.0))
                        .py(px(12.0))
                        .border_b_1()
                        .border_color(rgb(0x3A3A3C))
                        .child(div().text_xs().text_color(rgb(0x8E8E93)).child("DISKS")),
                )
                .children(services.iter().map(disk_row)),
        )
}
