use capitalize::Capitalize;
use gpui::prelude::*;
use gpui::*;

use crate::{
    app::{Crabdash, DockerAction, DockerFilter},
    components::common::{LucideIcon, lucide_icon},
};

use super::shared::{error_panel, placeholder_card};
use services::{ServiceItem, docker::Docker};

fn is_running_status(status: &str) -> bool {
    let normalized = status.to_ascii_lowercase();
    normalized.contains("running") || normalized.contains("healthy")
}

fn status_badge(status: &str, pending_action: Option<DockerAction>) -> Div {
    let label = pending_action
        .map(|action| action.pending_label())
        .unwrap_or(status);
    let is_running = pending_action.is_none() && is_running_status(status);
    let is_pending = pending_action.is_some();
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
        .child(label.to_string())
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
            this.docker_filter = filter;
            cx.notify();
        }))
}

fn action_button(
    cx: &mut Context<Crabdash>,
    cont: &ServiceItem,
    action: DockerAction,
    disabled: bool,
) -> impl IntoElement {
    let bg = rgb(0x242426);
    let disabled_bg = rgb(0x202022);
    let disabled_fg = rgb(0x6C6C70);
    let hover_bg = rgb(0x2F2F31);
    let cont_id = cont.id.clone();
    let cont_name = cont.name.clone();
    let button_id = SharedString::from(format!("{}-container-{cont_id}", action.command()));
    let icon = match action {
        DockerAction::Start => LucideIcon::Play,
        DockerAction::Stop => LucideIcon::X,
        DockerAction::Restart => LucideIcon::RefreshCw,
    };

    let button = div()
        .id(button_id)
        .h(px(34.0))
        .w(px(34.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(if disabled { disabled_bg } else { bg })
        .border_1()
        .border_color(if disabled { disabled_bg } else { rgb(0x2F2F31) })
        .rounded(px(8.0))
        .text_color(if disabled { disabled_fg } else { rgb(0xFFFFFF) })
        .child(lucide_icon(icon, 14.0));

    if disabled {
        button.cursor_default()
    } else {
        button
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .on_click(cx.listener(move |this, _, _, cx| {
                let machine_index = this.selected_machine;
                let mut machine = this.selected_machine().background_clone();

                let cont_id = cont_id.clone();
                let cont_name = cont_name.clone();

                if let Some(machine) = this.machine_store.machines.get_mut(machine_index) {
                    this.pending_docker_actions.insert(cont_id.clone(), action);

                    if let Some(container) = machine
                        .services
                        .docker
                        .iter_mut()
                        .find(|c| c.id.as_str() == cont_id.as_str())
                    {
                        container.error = None;
                    }
                }

                cx.notify();

                cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    let bg_cont_id = cont_id.clone();

                    async move {
                        let result = cx
                            .background_spawn(async move {
                                machine.container_action(&bg_cont_id, action.command())?;
                                machine.list_docker()
                            })
                            .await;

                        this.update(&mut cx, move |this, cx| {
                            this.pending_docker_actions.remove(&cont_id);

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
                                    let message = format!(
                                        "Failed to {} {cont_name}: {err}",
                                        action.command()
                                    );
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
            }))
    }
}

fn container_row(app: &Crabdash, cx: &mut Context<Crabdash>, service: &ServiceItem) -> Div {
    let pending_action = app.pending_docker_actions.get(&service.id).copied();
    let actions_disabled = pending_action.is_some();

    div()
        .w_full()
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x2F2F31))
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
                .child(action_button(
                    cx,
                    service,
                    DockerAction::Start,
                    actions_disabled,
                ))
                .child(action_button(
                    cx,
                    service,
                    DockerAction::Stop,
                    actions_disabled,
                ))
                .child(action_button(
                    cx,
                    service,
                    DockerAction::Restart,
                    actions_disabled,
                ))
                .child(
                    status_badge(&service.status.capitalize(), pending_action)
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
    let visible_services: Vec<ServiceItem> = match app.docker_filter {
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
                                    .iter()
                                    .map(|service| container_row(app, cx, service)),
                            )
                        }),
                ),
        )
}
