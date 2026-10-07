use super::{DockerRemoval, table::Column};
use crate::components::{common::control_tooltip, style};
use capitalize::Capitalize;
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use utils::container::Container;
use uuid::Uuid;

use crate::{
    app::Crabdash,
    components::{
        common::{LucideIcon, button, lucide_icon},
        scroll_list,
    },
};

use crate::components::table::{
    STATUS_WIDTH, clipped_text, filter_chip, fixed_column, placeholder_card, sort_heading,
    status_label, table_card, table_heading, table_row, toolbar,
};
use services::docker::{DockerAction, DockerFilter};

const DOCKER_ACTIONS_WIDTH: f32 = style::CONTROL * 5.0 + 16.0;

fn status_badge(container: &Container, pending_action: Option<DockerAction>) -> Div {
    let label = pending_action
        .map(|action| action.pending_label())
        .unwrap_or(&container.status);
    let is_running = pending_action.is_none() && container.is_running_status();
    let is_pending = pending_action.is_some();
    let status_fg = if container.is_paused() || is_pending {
        rgb(style::WARNING)
    } else if is_running {
        rgb(style::SUCCESS)
    } else if matches!(container.status.as_str(), "dead" | "unhealthy") {
        rgb(style::DANGER)
    } else {
        rgb(style::TEXT_MUTED)
    };
    status_label(label.to_string().capitalize(), status_fg)
}

fn stats_chip(
    id: impl Into<ElementId>,
    label: &str,
    value: String,
    active: bool,
    filter: DockerFilter,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    filter_chip(id, label, value.parse().unwrap_or(0), active).on_click(cx.listener(
        move |this, _, _, cx| {
            this.docker_table.filter = filter;
            this.docker_scroll_handle
                .set_offset(point(px(0.0), px(0.0)));
            cx.notify();
        },
    ))
}

fn action_button(
    cx: &mut Context<Crabdash>,
    machine_uuid: Uuid,
    container: &Container,
    action: DockerAction,
    disabled: bool,
) -> impl IntoElement {
    let id = container.id.clone();
    let disabled = disabled || !action.allowed_for(container);
    let button = div()
        .id(SharedString::from(format!(
            "{}-container-{id}",
            action.command()
        )))
        .size(gpui::rems(style::CONTROL / 16.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(style::RADIUS))
        .text_color(if disabled {
            rgb(0x606060)
        } else if matches!(action, DockerAction::Remove { .. }) {
            rgb(0xFF7068)
        } else {
            rgb(style::TEXT_PRIMARY)
        })
        .tooltip(move |_, cx| control_tooltip(action.label(), cx))
        .child(lucide_icon(action.icon(), style::ICON));
    if disabled {
        button.cursor_default()
    } else {
        button
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x303030)))
            .on_click(cx.listener(move |this, _, window, cx| {
                if matches!(action, DockerAction::Remove { .. }) {
                    let Some(machine) = this
                        .machine_store
                        .machines
                        .iter()
                        .find(|m| m.uuid == machine_uuid)
                    else {
                        return;
                    };
                    let Some(container) = machine.services.docker.iter().find(|c| c.id == id)
                    else {
                        return;
                    };
                    this.docker_removal = Some(DockerRemoval {
                        machine_uuid,
                        id: id.clone(),
                        name: container.name.clone(),
                        machine_name: machine.system_info.machine_name.clone(),
                        active: container.is_active_status(),
                        force: false,
                    });
                    this.focus_handle.focus(window);
                    cx.notify();
                } else {
                    this.execute_docker_action(machine_uuid, id.clone(), action, cx);
                }
            }))
    }
}

fn logs_button(
    cx: &mut Context<Crabdash>,
    machine_uuid: Uuid,
    container: &Container,
    logs_open: bool,
) -> impl IntoElement {
    let id = container.id.clone();
    let log_key = (machine_uuid, id.clone());
    let button_id = SharedString::from(format!("logs-container-{id}"));
    let bg = if logs_open {
        rgb(style::CONTROL_SELECTED_BG)
    } else {
        rgba(0x00000000)
    };

    div()
        .id(button_id)
        .h(gpui::rems(style::CONTROL / 16.0))
        .w(gpui::rems(style::CONTROL / 16.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .rounded(px(style::RADIUS))
        .text_color(rgb(style::TEXT_PRIMARY))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(style::SURFACE_HOVER)))
        .tooltip(|_, cx| control_tooltip("Show / hide logs", cx))
        .child(lucide_icon(Icon::FileText, style::ICON))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_docker_logs(log_key.clone(), cx);
        }))
}

fn container_row_card(
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
    container: &Container,
    logs_open: bool,
    show_id: bool,
) -> impl IntoElement {
    let pending_action = app
        .pending_docker_actions
        .get(&(app.selected_machine().uuid, container.id.clone()))
        .copied();
    let actions_disabled = pending_action.is_some();

    table_row()
        .id(SharedString::from(format!(
            "container-row-{}",
            container.id
        )))
        .hover(|s| s.bg(rgb(style::SURFACE_HOVER)))
        .bg(rgb(style::SURFACE))
        .border_b_1()
        .border_color(rgb(style::BORDER))
        .py(px(12.0))
        .child(
            div().flex_1().min_w_0().child(
                clipped_text(container.name.clone())
                    .w_full()
                    .text_size(gpui::rems(style::TEXT / 16.0))
                    .text_color(rgb(style::TEXT_PRIMARY)),
            ),
        )
        .when(show_id, |this| {
            this.child(
                fixed_column(160.0).child(
                    clipped_text(container.id.clone())
                        .w_full()
                        .text_size(gpui::rems(style::META / 16.0))
                        .text_color(rgb(style::TEXT_MUTED)),
                ),
            )
        })
        .child(
            div()
                .w(gpui::rems(DOCKER_ACTIONS_WIDTH / 16.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(4.0))
                .child(logs_button(
                    cx,
                    app.selected_machine().uuid,
                    container,
                    logs_open,
                ))
                .child(action_button(
                    cx,
                    app.selected_machine().uuid,
                    container,
                    if container.is_active_status() {
                        DockerAction::Stop
                    } else {
                        DockerAction::Start
                    },
                    actions_disabled,
                ))
                .child(action_button(
                    cx,
                    app.selected_machine().uuid,
                    container,
                    DockerAction::Restart,
                    actions_disabled,
                ))
                .child(action_button(
                    cx,
                    app.selected_machine().uuid,
                    container,
                    if container.is_paused() {
                        DockerAction::Unpause
                    } else {
                        DockerAction::Pause
                    },
                    actions_disabled,
                ))
                .child(action_button(
                    cx,
                    app.selected_machine().uuid,
                    container,
                    DockerAction::Remove { force: false },
                    actions_disabled,
                )),
        )
        .child(fixed_column(STATUS_WIDTH).child(status_badge(container, pending_action)))
}

fn container_row(
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
    container: &Container,
    show_id: bool,
) -> Div {
    let container_id = container.id.clone();
    let log_key = (app.selected_machine().uuid, container_id.clone());
    let logs_open = app.logs_open_containers.contains(&log_key);

    let state = app.expanded_docker_logs.get(&log_key);

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(container_row_card(app, cx, container, logs_open, show_id))
        .when(logs_open, |d| {
            d.child(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(10.0))
                    .bg(rgb(0x181818))
                    .child(match state {
                        Some(state) if state.loaded => {
                            crate::features::terminal::render_view(&state.rendered, cx)
                        }
                        _ => div()
                            .text_size(rems(style::META / 16.0))
                            .child("Loading logs..."),
                    }),
            )
        })
}

fn table_header(show_id: bool, app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    table_heading()
        .child(
            div().flex_1().min_w_0().h_full().child(
                sort_heading(
                    "docker-sort-name",
                    "Container",
                    app.docker_table.sort.indicator(Column::Name),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.docker_table.sort.select(Column::Name);
                    this.docker_scroll_handle
                        .set_offset(point(px(0.0), px(0.0)));
                    cx.notify();
                })),
            ),
        )
        .when(show_id, |this| {
            this.child(
                fixed_column(160.0).h_full().child(
                    sort_heading(
                        "docker-sort-id",
                        "ID",
                        app.docker_table.sort.indicator(Column::Id),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.docker_table.sort.select(Column::Id);
                        this.docker_scroll_handle
                            .set_offset(point(px(0.0), px(0.0)));
                        cx.notify();
                    })),
                ),
            )
        })
        .child(
            div()
                .w(rems(DOCKER_ACTIONS_WIDTH / 16.0))
                .flex_none()
                .text_center()
                .child("Actions"),
        )
        .child(
            fixed_column(STATUS_WIDTH).h_full().child(
                sort_heading(
                    "docker-sort-status",
                    "Status",
                    app.docker_table.sort.indicator(Column::Status),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.docker_table.sort.select(Column::Status);
                    this.docker_scroll_handle
                        .set_offset(point(px(0.0), px(0.0)));
                    cx.notify();
                }))
                .justify_center(),
            ),
        )
}

pub fn render(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> Div {
    let show_id = window.viewport_size().width
        - if app.sidebar_collapsed {
            px(0.0)
        } else {
            app.sidebar_width
        }
        >= px(640.0 * app.preferences.interface_font_size / 13.0);
    let machine = app.selected_machine();
    let containers = machine.services.docker.clone();

    let total_count = containers.len();
    let running_count = containers
        .iter()
        .filter(|container| container.is_running_status())
        .count();
    let paused_count = containers.iter().filter(|c| c.is_paused()).count();
    let stopped_count = containers.iter().filter(|c| !c.is_active_status()).count();
    let visible_services = app
        .docker_table
        .visible(&containers, &app.docker_table.search.query(cx));

    scroll_list::render(
        "docker-scroll",
        &app.docker_scroll_handle,
        Some({
            toolbar(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div().flex().child(
                            button(
                                "run-container-open",
                                Some(LucideIcon::Play),
                                Some("Run"),
                                true,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_docker_run_modal(cx);
                            })),
                        ),
                    )
                    .child(stats_chip(
                        "docker-filter-total",
                        "All",
                        total_count.to_string(),
                        app.docker_table.filter == DockerFilter::Total,
                        DockerFilter::Total,
                        cx,
                    ))
                    .child(stats_chip(
                        "docker-filter-running",
                        "Running",
                        running_count.to_string(),
                        app.docker_table.filter == DockerFilter::Running,
                        DockerFilter::Running,
                        cx,
                    ))
                    .child(stats_chip(
                        "docker-filter-paused",
                        "Paused",
                        paused_count.to_string(),
                        app.docker_table.filter == DockerFilter::Paused,
                        DockerFilter::Paused,
                        cx,
                    ))
                    .child(stats_chip(
                        "docker-filter-stopped",
                        "Stopped",
                        stopped_count.to_string(),
                        app.docker_table.filter == DockerFilter::Stopped,
                        DockerFilter::Stopped,
                        cx,
                    )),
                &app.docker_table.search,
            )
            .into_any_element()
        }),
        div()
            .flex()
            .flex_col()
            .when(total_count == 0, |this| {
                this.child(placeholder_card(
                    "No containers",
                    "No containers have been loaded for this machine yet.",
                ))
            })
            .when(total_count > 0 && visible_services.is_empty(), |this| {
                this.child(placeholder_card(
                    "No matching containers",
                    "Try another status filter or clear the search field.",
                ))
            })
            .when(!visible_services.is_empty(), |this| {
                this.child(
                    table_card().child(table_header(show_id, app, cx)).children(
                        visible_services
                            .iter()
                            .map(|service| container_row(app, cx, service, show_id)),
                    ),
                )
            }),
        cx,
    )
}
