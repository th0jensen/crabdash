use gpui::prelude::*;
use gpui::*;

use crate::app::Crabdash;

use super::{error_panel, placeholder_card, stats_chip, status_badge};
use services::ServiceItem;

fn system_service_row(service: &ServiceItem) -> Div {
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
        .filter(|service| service.status.contains("running") || service.status.contains("active"))
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
                .child(stats_chip("Running", running_count.to_string())),
        )
        .child(
            div()
                .id("services-scroll")
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
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x8E8E93))
                                .child("SYSTEM SERVICES"),
                        ),
                )
                .children(services.iter().map(system_service_row)),
        )
}
