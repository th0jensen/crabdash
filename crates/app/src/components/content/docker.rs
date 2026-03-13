use capitalize::Capitalize;
use gpui::prelude::*;
use gpui::*;

use crate::{
    app::Crabdash,
    components::common::{LucideIcon, button},
};

use super::{error_panel, placeholder_card, stats_chip, status_badge};
use services::{ServiceItem, docker::Docker};

fn action_button(
    cx: &mut Context<Crabdash>,
    cont: &mut ServiceItem,
    action: String,
) -> impl IntoElement {
    let cont_id = cont.id.clone();
    let cont_name = cont.name.clone();
    let button_id = SharedString::from(format!("{action}-container-{cont_id}"));
    let label = SharedString::from(action.capitalize());

    let icon = match action.as_str() {
        "start" => LucideIcon::Play,
        "stop" => LucideIcon::X,
        "restart" => LucideIcon::RefreshCw,
        _ => LucideIcon::Circle,
    };

    button(button_id, icon, label, false).on_click(cx.listener(move |this, _, _, cx| {
        let machine_index = this.selected_machine;
        let mut machine = this.selected_machine().background_clone();

        let action = action.clone();
        let cont_id = cont_id.clone();
        let cont_name = cont_name.clone();

        cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();

            let bg_action = action.clone();
            let bg_cont_id = cont_id.clone();
            let bg_cont_name = cont_name.clone();

            async move {
                let result = cx
                    .background_spawn(async move {
                        eprintln!("{bg_action}ing Docker container {bg_cont_name} ({bg_cont_id})");
                        machine.container_action(&bg_cont_id, &bg_action)?;
                        machine.list_docker()
                    })
                    .await;

                this.update(&mut cx, move |this, cx| {
                    match result {
                        Ok(services) => {
                            eprintln!("{action}ed Docker container {cont_name} ({cont_id})");

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
    let mut services = machine.services.docker.clone();

    if let Some(error) = machine
        .services
        .docker_error
        .as_ref()
        .map(|error| error.to_string())
    {
        return error_panel("Unable to load Docker", error);
    }

    if services.is_empty() {
        return placeholder_card(
            "DOCKER CONTAINERS",
            "No containers have been loaded for this machine yet.",
        );
    }

    let running_count = services
        .iter()
        .filter(|service| service.status.contains("running") || service.status.contains("healthy"))
        .count();

    div()
        .flex()
        .flex_col()
        .h_full()
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
                .flex_1()
                .overflow_y_scroll()
                .child(
                    div().flex().flex_col().gap(px(8.0)).children(
                        services
                            .iter_mut()
                            .map(|service| container_row(cx, service)),
                    ),
                ),
        )
}
