use gpui::prelude::*;
use gpui::*;

use crate::app::{Crabdash, MainTab};
use crate::components::common::button;
use machines::machine::Machine;
use services::ServiceItem;

fn tab_button(tab: MainTab, active: bool, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let bg = if active { rgb(0x2C2C2E) } else { rgb(0x1C1C1E) };
    let color = if active { rgb(0xFFFFFF) } else { rgb(0xAEAEB2) };

    div()
        .id(SharedString::from(format!(
            "tab-{}",
            tab.label().to_lowercase()
        )))
        .flex_1()
        .h(px(34.0))
        .bg(bg)
        .border_r_1()
        .border_color(rgb(0x3A3A3C))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2A2C)))
        .child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(color)
                .child(tab.label().to_string()),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.active_tab = tab;
            this.refresh_services();
            cx.notify();
        }))
}

fn refresh_button(cx: &mut Context<Crabdash>) -> impl IntoElement {
    button("refresh-button", "Refresh", false).on_click(cx.listener(|this, _, _, cx| {
        this.refresh_services();
        cx.notify();
    }))
}

fn service_row(service: &ServiceItem) -> Div {
    let is_running = service.status.contains("running") || service.status.contains("healthy");
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
        .child(
            div()
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(999.0))
                .bg(status_bg)
                .text_xs()
                .text_color(status_fg)
                .child(service.status.clone()),
        )
}

fn stats_chip(label: &str, value: String) -> Div {
    div()
        .px(px(10.0))
        .py(px(8.0))
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x8E8E93))
                .child(label.to_string()),
        )
        .child(div().text_sm().text_color(white()).child(value))
}

fn placeholder_card(title: &str, description: &str) -> Div {
    div()
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(div().text_sm().text_color(white()).child(title.to_string()))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xAEAEB2))
                .child(description.to_string()),
        )
}

fn services_panel(
    title: &str,
    error_title: &str,
    services: &[ServiceItem],
    error: Option<String>,
) -> Div {
    let running_count = services
        .iter()
        .filter(|service| service.status.contains("running") || service.status.contains("healthy"))
        .count();

    if let Some(error) = error {
        return div()
            .bg(rgb(0x47232B))
            .border_1()
            .border_color(rgb(0x5A2D35))
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xFF9F99))
                    .child(error_title.to_string()),
            )
            .child(div().text_sm().text_color(rgb(0xF2B6B2)).child(error));
    }

    if services.is_empty() {
        return placeholder_card(title, "No items have been loaded for this machine yet.");
    }

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
                .id("docker-scroll")
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
                                .child(title.to_string()),
                        ),
                )
                .children(services.iter().map(service_row)),
        )
}

fn active_services<'a>(machine: &'a Machine, tab: MainTab) -> (&'a [ServiceItem], Option<String>) {
    match tab {
        MainTab::Docker => (
            &machine.services.docker,
            machine
                .services
                .docker_error
                .as_ref()
                .map(|error| error.to_string()),
        ),
        MainTab::Disks => (
            &machine.services.disks,
            machine.services.disks_error.clone(),
        ),
        MainTab::Services => (
            &machine.services.systemd,
            machine.services.systemd_error.clone(),
        ),
    }
}

fn active_panel(app: &Crabdash) -> Div {
    let machine = app.selected_machine();
    let (services, error) = active_services(machine, app.active_tab);

    match app.active_tab {
        MainTab::Docker => services_panel(
            "DOCKER CONTAINERS",
            "Unable to load Docker",
            services,
            error,
        ),
        MainTab::Disks => services_panel("DISKS", "Unable to load disks", services, error),
        MainTab::Services => services_panel(
            "SYSTEM SERVICES",
            "Unable to load services",
            services,
            error,
        ),
    }
}

pub fn render_title_bar(app: &Crabdash) -> Div {
    let selected_machine = app.selected_machine();

    div()
        .h(px(38.0))
        .px(px(14.0))
        .border_b_1()
        .border_color(rgb(0x2A2A2C))
        .bg(rgb(0x18181A))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .pl(px(72.0))
                .text_sm()
                .text_color(rgb(0x8E8E93))
                .child("crabdash"),
        )
        .child(
            div()
                .px(px(10.0))
                .py(px(5.0))
                .text_sm()
                .text_color(rgb(0xE5E5EA))
                .child(format!(
                    "{} | {}",
                    selected_machine.system_info.machine_name,
                    selected_machine.system_info.os_version
                )),
        )
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .bg(rgb(0x1E1E20))
        .child(
            div()
                .h(px(34.0))
                .border_b_1()
                .border_color(rgb(0x3A3A3C))
                .flex()
                .child(tab_button(
                    MainTab::Docker,
                    app.active_tab == MainTab::Docker,
                    cx,
                ))
                .child(tab_button(
                    MainTab::Disks,
                    app.active_tab == MainTab::Disks,
                    cx,
                ))
                .child(tab_button(
                    MainTab::Services,
                    app.active_tab == MainTab::Services,
                    cx,
                )),
        )
        .child(
            div()
                .id("content-scroll")
                .flex_1()
                .overflow_y_scroll()
                .bg(rgb(0x1E1E20))
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(20.0))
                        .py(px(14.0))
                        .border_b_1()
                        .border_color(rgb(0x3A3A3C))
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .text_xl()
                                        .text_color(white())
                                        .child(app.active_tab.label()),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(0x8E8E93))
                                        .child(app.active_tab.subtitle()),
                                ),
                        )
                        .child(if app.active_tab == MainTab::Docker {
                            refresh_button(cx).into_any_element()
                        } else {
                            div().into_any_element()
                        }),
                )
                .child(div().p(px(20.0)).child(active_panel(app))),
        )
}
