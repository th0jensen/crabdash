use capitalize::Capitalize;
use gpui::prelude::*;
use gpui::*;

use crate::{
    app::{Crabdash, DockerFilter},
    components::common::{LucideIcon, button},
};

use super::shared::{error_panel, placeholder_card};
use services::{ServiceItem, docker::Docker};

fn is_running_status(status: &str) -> bool {
    let normalized = status.to_ascii_lowercase();
    normalized.contains("running") || normalized.contains("healthy")
}

fn status_badge(status: &str) -> Div {
    let normalized = status.to_ascii_lowercase();
    let is_running = is_running_status(status);
    let is_pending = matches!(normalized.as_str(), "starting" | "stopping" | "restarting");
    let status_bg = if is_running {
        rgb(0x193D2A)
    } else if is_pending {
        rgb(0x473B1F)
    } else {
        rgb(0x47232B)
    };
    let status_fg = if is_running {
        rgb(0x30D158)
    } else if is_pending {
        rgb(0xFFD60A)
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
    filter: DockerFilter,
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
        .border_color(rgb(0x3A3A3C))
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
            this.docker_filter = filter;
            cx.notify();
        }))
}

fn action_status(action: &str) -> &'static str {
    match action {
        "start" => "Starting",
        "stop" => "Stopping",
        "restart" => "Restarting",
        _ => "Working",
    }
}

fn action_button(
    cx: &mut Context<Crabdash>,
    cont: &mut ServiceItem,
    action: String,
) -> impl IntoElement {
    let cont_id = cont.id.clone();
    let cont_name = cont.name.clone();
    let current_status = cont.status.clone();
    let button_id = SharedString::from(format!("{action}-container-{cont_id}"));
    let icon = match action.as_str() {
        "start" => LucideIcon::Play,
        "stop" => LucideIcon::X,
        "restart" => LucideIcon::RefreshCw,
        _ => LucideIcon::Circle,
    };

    button(button_id, icon, None::<SharedString>, false).on_click(cx.listener(
        move |this, _, _, cx| {
            let machine_index = this.selected_machine;
            let mut machine = this.selected_machine().background_clone();

            let action = action.clone();
            let cont_id = cont_id.clone();
            let cont_name = cont_name.clone();
            let previous_status = current_status.clone();
            let pending_status = action_status(&action).to_string();

            if let Some(machine) = this.machine_store.machines.get_mut(machine_index) {
                if let Some(container) = machine
                    .services
                    .docker
                    .iter_mut()
                    .find(|c| c.id.as_str() == cont_id.as_str())
                {
                    container.status = pending_status;
                    container.error = None;
                }
            }

            cx.notify();

            cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();

                let bg_action = action.clone();
                let bg_cont_id = cont_id.clone();

                async move {
                    let result = cx
                        .background_spawn(async move {
                            machine.container_action(&bg_cont_id, &bg_action)?;
                            machine.list_docker()
                        })
                        .await;

                    this.update(&mut cx, move |this, cx| {
                        match result {
                            Ok(services) => {
                                if let Some(machine) =
                                    this.machine_store.machines.get_mut(machine_index)
                                {
                                    machine.services.docker = services;
                                    machine.services.docker_error = None;
                                }

                                this.clear_status_message();
                            }
                            Err(err) => {
                                let message = format!("Failed to {action} {cont_name}: {err}");
                                eprintln!("{message}");
                                this.set_status_error(message.clone());

                                if let Some(machine) =
                                    this.machine_store.machines.get_mut(machine_index)
                                {
                                    if let Some(container) = machine
                                        .services
                                        .docker
                                        .iter_mut()
                                        .find(|c| c.id.as_str() == cont_id.as_str())
                                    {
                                        container.status = previous_status.clone();
                                        container.error = Some(message);
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
        },
    ))
}

fn container_row(cx: &mut Context<Crabdash>, service: &mut ServiceItem) -> Div {
    div()
        .w_full()
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .rounded(px(8.0))
        .px(px(14.0))
        .py(px(12.0))
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
                    "ID: {}",
                    // service.kind.label(),
                    service.id
                ))),
        )
        .child(
            // Keep trailing controls grouped here so per-container actions can
            // be added beside the live status badge without reshaping the row.
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(action_button(cx, service, "start".to_string()))
                .child(action_button(cx, service, "stop".to_string()))
                .child(action_button(cx, service, "restart".to_string()))
                .child(
                    status_badge(&service.status.capitalize())
                        .w(px(80.0))
                        .text_center(),
                ),
        )
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let machine = app.selected_machine();
    let services = machine.services.docker.clone();

    if let Some(error) = machine
        .services
        .docker_error
        .as_ref()
        .map(|error| error.to_string())
    {
        return error_panel("Unable to load Docker", error);
    }

    let total_count = services.len();
    let running_count = services
        .iter()
        .filter(|service| is_running_status(&service.status))
        .count();
    let mut visible_services: Vec<ServiceItem> = match app.docker_filter {
        DockerFilter::Total => services,
        DockerFilter::Running => services
            .into_iter()
            .filter(|service| is_running_status(&service.status))
            .collect(),
    };

    div()
        .flex()
        .flex_col()
        .h_full()
        .gap(px(12.0))
        .when(total_count > 0, |this| {
            this.child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(stats_chip(
                        "docker-filter-total",
                        "Total",
                        total_count.to_string(),
                        app.docker_filter == DockerFilter::Total,
                        DockerFilter::Total,
                        cx,
                    ))
                    .child(stats_chip(
                        "docker-filter-running",
                        "Running",
                        running_count.to_string(),
                        app.docker_filter == DockerFilter::Running,
                        DockerFilter::Running,
                        cx,
                    )),
            )
        })
        .child(
            div()
                .id("docker-scroll")
                .flex_1()
                .overflow_y_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .when(total_count == 0, |this| {
                            this.child(placeholder_card(
                                "No Containers Found",
                                "No containers have been loaded for this machine yet.",
                            ))
                        })
                        .when(total_count > 0 && visible_services.is_empty(), |this| {
                            this.child(placeholder_card(
                                "No Running Containers",
                                "No running containers match the current filter.",
                            ))
                        })
                        .when(!visible_services.is_empty(), |this| {
                            this.children(
                                visible_services
                                    .iter_mut()
                                    .map(|service| container_row(cx, service)),
                            )
                        }),
                ),
        )
}
