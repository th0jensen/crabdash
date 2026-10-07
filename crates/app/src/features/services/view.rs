use crate::components::{common::control_tooltip, style};
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use services::ServiceAction;
use utils::services::ServiceItem;

use crate::{
    app::Crabdash,
    components::{common::lucide_icon, scroll_list},
};

use super::table::{Column, Filter, metadata as service_metadata};
use crate::components::table::{
    ACTIONS_WIDTH, STATUS_WIDTH, clipped_text, error_panel, filter_chip, fixed_column,
    placeholder_card, sort_heading, status_label, table_card, table_heading, table_row, toolbar,
};

fn status_badge(service: &ServiceItem, pending_action: Option<ServiceAction>) -> Div {
    let label = pending_action
        .map(|action| action.pending_label())
        .unwrap_or_else(|| service.status_label());
    let is_running = pending_action.is_none() && service.is_running();
    let is_pending = pending_action.is_some();
    let is_failed = label == "Failed";
    let status_fg = if is_running {
        rgb(style::SUCCESS)
    } else if is_pending {
        rgb(style::WARNING)
    } else if is_failed {
        rgb(style::DANGER)
    } else {
        rgb(style::TEXT_MUTED)
    };
    status_label(label, status_fg)
}

fn stats_chip(
    id: impl Into<ElementId>,
    label: &str,
    value: String,
    active: bool,
    filter: Filter,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    filter_chip(id, label, value.parse().unwrap_or(0), active).on_click(cx.listener(
        move |this, _, _, cx| {
            this.services_table.filter = filter;
            this.services_scroll_handle
                .set_offset(point(px(0.0), px(0.0)));
            cx.notify();
        },
    ))
}

fn table_header(show_details: bool, app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    table_heading()
        .child(
            div().flex_1().min_w_0().h_full().child(
                sort_heading(
                    "services-sort-name",
                    "Service",
                    app.services_table.sort.indicator(Column::Name),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.services_table.sort.select(Column::Name);
                    this.services_scroll_handle
                        .set_offset(point(px(0.0), px(0.0)));
                    cx.notify();
                })),
            ),
        )
        .when(show_details, |this| {
            this.child(
                fixed_column(200.0).h_full().child(
                    sort_heading(
                        "services-sort-details",
                        "Details",
                        app.services_table.sort.indicator(Column::Details),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.services_table.sort.select(Column::Details);
                        this.services_scroll_handle
                            .set_offset(point(px(0.0), px(0.0)));
                        cx.notify();
                    })),
                ),
            )
        })
        .child(fixed_column(ACTIONS_WIDTH).text_center().child("Actions"))
        .child(
            fixed_column(STATUS_WIDTH).h_full().child(
                sort_heading(
                    "services-sort-status",
                    "Status",
                    app.services_table.sort.indicator(Column::Status),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.services_table.sort.select(Column::Status);
                    this.services_scroll_handle
                        .set_offset(point(px(0.0), px(0.0)));
                    cx.notify();
                }))
                .justify_center(),
            ),
        )
}

fn service_action_button(
    cx: &mut Context<Crabdash>,
    service: &ServiceItem,
    action: ServiceAction,
    disabled: bool,
) -> impl IntoElement {
    let bg = rgba(0x00000000);
    let disabled_bg = rgba(0x00000000);
    let disabled_fg = rgb(0x606060);
    let hover_bg = rgb(0x303030);

    let name = service.name.clone();
    let button_id = SharedString::from(format!("{}-service-{name}", action.command()));

    let button = div()
        .id(button_id)
        .h(gpui::rems(style::CONTROL / 16.0))
        .w(gpui::rems(style::CONTROL / 16.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(if disabled { disabled_bg } else { bg })
        .rounded(px(style::RADIUS))
        .text_color(if disabled { disabled_fg } else { rgb(0xFFFFFF) })
        .tooltip(move |_, cx| control_tooltip(format!("{}", action.command()), cx))
        .child(lucide_icon(action.icon(), style::ICON));

    if disabled {
        button.cursor_default()
    } else {
        button
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.execute_service_action(name.clone(), action, cx);
            }))
    }
}

fn service_logs_button(
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
    service: &ServiceItem,
) -> impl IntoElement {
    let service_name = service.name.clone();
    let log_key = (app.selected_machine().uuid, service_name.clone());
    let logs_open = app.logs_open_services.contains(&log_key);
    let bg = if logs_open {
        rgb(style::CONTROL_SELECTED_BG)
    } else {
        rgba(0x00000000)
    };

    div()
        .id(SharedString::from(format!("logs-service-{service_name}")))
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
            this.toggle_service_logs(log_key.clone(), cx);
        }))
}

fn system_service_row(
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
    service: &ServiceItem,
    show_details: bool,
) -> Div {
    let service_name = service.name.clone();
    let pending_action = app.pending_service_actions.get(&service_name).copied();
    let actions_disabled = pending_action.is_some();
    let log_key = (app.selected_machine().uuid, service_name.clone());
    let logs_open = app.logs_open_services.contains(&log_key);
    let state = app.expanded_service_logs.get(&log_key);

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            table_row()
                .id(SharedString::from(format!("service-row-{}", service.name)))
                .hover(|s| s.bg(rgb(style::SURFACE_HOVER)))
                .py(px(10.0))
                .border_b_1()
                .border_color(rgb(style::BORDER))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            clipped_text(service.name.clone())
                                .w_full()
                                .text_size(gpui::rems(style::TEXT / 16.0))
                                .text_color(rgb(style::TEXT_PRIMARY)),
                        )
                        .when_some(
                            service.description.as_ref().filter(|description| {
                                !description.trim().is_empty()
                                    && description.as_str() != service.name
                            }),
                            |this, description| {
                                this.child(
                                    clipped_text(description.clone())
                                        .w_full()
                                        .text_size(gpui::rems(style::META / 16.0))
                                        .text_color(rgb(style::TEXT_MUTED)),
                                )
                            },
                        ),
                )
                .when(show_details, |this| {
                    this.child(
                        fixed_column(200.0).child(
                            clipped_text(service_metadata(service))
                                .w_full()
                                .text_size(gpui::rems(style::META / 16.0))
                                .text_color(rgb(style::TEXT_MUTED)),
                        ),
                    )
                })
                .child(
                    fixed_column(ACTIONS_WIDTH)
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(4.0))
                        .child(service_logs_button(app, cx, service))
                        .child(service_action_button(
                            cx,
                            service,
                            if service.is_running() {
                                ServiceAction::Stop
                            } else {
                                ServiceAction::Start
                            },
                            actions_disabled,
                        ))
                        .child(service_action_button(
                            cx,
                            service,
                            ServiceAction::Restart,
                            actions_disabled,
                        )),
                )
                .child(fixed_column(STATUS_WIDTH).child(status_badge(service, pending_action))),
        )
        .when(logs_open, |this| {
            this.child(crate::features::logs::render(
                format!(
                    "service-logs-{}-{service_name}",
                    app.selected_machine().uuid
                ),
                state,
                cx,
            ))
        })
}

pub fn render(app: &Crabdash, window: &Window, cx: &mut Context<Crabdash>) -> Div {
    let show_details = window.viewport_size().width
        - if app.sidebar_collapsed {
            px(0.0)
        } else {
            app.sidebar_width
        }
        >= px(720.0 * app.preferences.interface_font_size / 13.0);
    let machine = app.selected_machine();
    let services = machine.services.systemd.clone();

    if let Some(error) = machine.services.systemd_error.clone() {
        return error_panel("Unable to load services", error);
    }

    let total_count = services.len();
    let running_count = services
        .iter()
        .filter(|s| Filter::Active.matches(s))
        .count();
    let inactive_count = services
        .iter()
        .filter(|s| Filter::Inactive.matches(s))
        .count();
    let failed_count = services
        .iter()
        .filter(|s| Filter::Failed.matches(s))
        .count();
    let visible_services = app
        .services_table
        .visible(&services, &app.services_table.search.query(cx));

    scroll_list::render(
        "services-scroll",
        &app.services_scroll_handle,
        Some(
            toolbar(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(stats_chip(
                        "service-filter-total",
                        "All",
                        total_count.to_string(),
                        app.services_table.filter == Filter::All,
                        Filter::All,
                        cx,
                    ))
                    .child(stats_chip(
                        "service-filter-active",
                        "Active",
                        running_count.to_string(),
                        app.services_table.filter == Filter::Active,
                        Filter::Active,
                        cx,
                    ))
                    .child(stats_chip(
                        "service-filter-inactive",
                        "Inactive",
                        inactive_count.to_string(),
                        app.services_table.filter == Filter::Inactive,
                        Filter::Inactive,
                        cx,
                    ))
                    .child(stats_chip(
                        "service-filter-failed",
                        "Failed",
                        failed_count.to_string(),
                        app.services_table.filter == Filter::Failed,
                        Filter::Failed,
                        cx,
                    )),
                &app.services_table.search,
            )
            .into_any_element(),
        ),
        div()
            .flex()
            .flex_col()
            .when(total_count == 0, |this| {
                this.child(placeholder_card(
                    "No services",
                    "No system services have been loaded for this machine yet.",
                ))
            })
            .when(total_count > 0 && visible_services.is_empty(), |this| {
                this.child(placeholder_card(
                    "No matching services",
                    "Try another status filter or clear the search field.",
                ))
            })
            .when(!visible_services.is_empty(), |this| {
                this.child(
                    table_card()
                        .child(table_header(show_details, app, cx))
                        .children(
                            visible_services
                                .iter()
                                .map(|service| system_service_row(app, cx, service, show_details)),
                        ),
                )
            }),
        cx,
    )
}
