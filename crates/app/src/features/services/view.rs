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
    ACTIONS_WIDTH, clipped_text, error_panel, filter_chip, fixed_column, placeholder_card,
    responsive_row, responsive_status_column, sort_heading, status_label, table_heading, toolbar,
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
            this.services_table.list.scroll_to_top();
            cx.notify();
        },
    ))
}

fn table_header(
    show_details: bool,
    compact: bool,
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
) -> Div {
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
                    this.services_table.list.scroll_to_top();
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
                        this.services_table.list.scroll_to_top();
                        cx.notify();
                    })),
                ),
            )
        })
        .when(!compact, |this| {
            this.child(fixed_column(ACTIONS_WIDTH).text_center().child("Actions"))
        })
        .child(
            responsive_status_column(compact).h_full().child(
                sort_heading(
                    "services-sort-status",
                    "Status",
                    app.services_table.sort.indicator(Column::Status),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.services_table.sort.select(Column::Status);
                    this.services_table.list.scroll_to_top();
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
    compact: bool,
) -> Div {
    let service_name = service.name.clone();
    let pending_action = app
        .pending_service_actions
        .get(&(app.selected_machine().uuid, service_name.clone()))
        .copied();
    let actions_disabled = pending_action.is_some();
    let log_key = (app.selected_machine().uuid, service_name.clone());
    let logs_open = app.logs_open_services.contains(&log_key);
    let state = app.expanded_service_logs.get(&log_key);

    let identity = div()
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
                !description.trim().is_empty() && description.as_str() != service.name
            }),
            |this, description| {
                this.child(
                    clipped_text(description.clone())
                        .w_full()
                        .text_size(gpui::rems(style::META / 16.0))
                        .text_color(rgb(style::TEXT_MUTED)),
                )
            },
        );
    let detail = show_details.then(|| {
        fixed_column(200.0)
            .child(
                clipped_text(service_metadata(service))
                    .w_full()
                    .text_size(rems(style::META / 16.0))
                    .text_color(rgb(style::TEXT_MUTED)),
            )
            .into_any_element()
    });
    let actions = fixed_column(ACTIONS_WIDTH)
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
        ));
    div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            responsive_row(
                compact,
                identity,
                detail,
                actions,
                responsive_status_column(compact).child(status_badge(service, pending_action)),
            )
            .id(SharedString::from(format!("service-row-{}", service.name)))
            .hover(|s| s.bg(rgb(style::SURFACE_HOVER)))
            .py(px(10.0))
            .border_b_1()
            .border_color(rgb(style::BORDER)),
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

pub fn render(
    app: &Crabdash,
    _window: &Window,
    cx: &mut Context<Crabdash>,
    panel_width: Pixels,
) -> Div {
    let compact = panel_width
        < px(400.0 * app.preferences.interface_font_size / crate::components::style::TEXT);
    let show_details = panel_width
        >= px(720.0 * app.preferences.interface_font_size / crate::components::style::TEXT);
    let machine = app.selected_machine();
    let services = &machine.services.systemd;

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
        .visible(services, &app.services_table.search.query(cx));

    let settings = crate::features::preferences::current(cx);
    // Only the height is needed here; resolving terminal glyph advances for
    // every sample would add work even when every log panel is collapsed.
    let line_height = (settings.terminal_font_size * settings.terminal_line_height).ceil();
    let rows = visible_services
        .iter()
        .map(|service| {
            let key = (machine.uuid, service.name.clone());
            super::virtual_list::Row {
                name: service.name.clone(),
                description: service.description.as_ref().is_some_and(|description| {
                    !description.trim().is_empty() && description.as_str() != service.name
                }),
                logs: app.logs_open_services.contains(&key).then(|| {
                    f32::from(crate::features::logs::content_height(
                        app.expanded_service_logs.get(&key),
                        line_height,
                    ))
                    .to_bits()
                }),
            }
        })
        .collect();
    let target = (machine.uuid, app.machine_selection_generation);
    app.services_table.list.prepare(
        target,
        rows,
        super::virtual_list::Metrics {
            compact,
            interface_font: app.preferences.interface_font.clone(),
            interface_size: app.preferences.interface_font_size.to_bits(),
            log_line_height: line_height.to_bits(),
        },
    );

    let body = if total_count == 0 {
        placeholder_card(
            "No services",
            "No system services have been loaded for this machine yet.",
        )
    } else if visible_services.is_empty() {
        placeholder_card(
            "No matching services",
            "Try another status filter or clear the search field.",
        )
    } else {
        let snapshot: Vec<ServiceItem> = visible_services.into_iter().cloned().collect();
        let count = snapshot.len();
        let owner = cx.entity().downgrade();
        let list = list(
            app.services_table.list.handle.clone(),
            move |index, _, cx| {
                if index == count + 1 {
                    return div().h(px(32.0)).into_any_element();
                }
                let rendered = owner.update(cx, |app, cx| {
                    // A callback must never render a captured row for a new target.
                    if (
                        app.selected_machine().uuid,
                        app.machine_selection_generation,
                    ) != target
                    {
                        return div().into_any_element();
                    }
                    if index == 0 {
                        div()
                            .w_full()
                            .overflow_hidden()
                            .bg(rgb(style::SURFACE))
                            .border_t_1()
                            .border_l_1()
                            .border_r_1()
                            .border_color(rgb(style::BORDER))
                            .rounded_t(px(style::CARD_RADIUS))
                            .child(table_header(show_details, compact, app, cx))
                            .into_any_element()
                    } else if let Some(service) = snapshot.get(index - 1) {
                        div()
                            .w_full()
                            .overflow_hidden()
                            .bg(rgb(style::SURFACE))
                            .border_l_1()
                            .border_r_1()
                            .border_color(rgb(style::BORDER))
                            .when(index == count, |this| {
                                this.border_b_1().rounded_b(px(style::CARD_RADIUS))
                            })
                            .child(system_service_row(app, cx, service, show_details, compact))
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    }
                });
                match rendered {
                    Ok(element) => element,
                    Err(_) => div().into_any_element(),
                }
            },
        )
        .size_full();
        div().size_full().min_h_0().min_w_0().child(list)
    };

    scroll_list::bounded(
        toolbar(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_wrap()
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
            panel_width
                < px(620.0 * app.preferences.interface_font_size / crate::components::style::TEXT),
        )
        .into_any_element(),
        body,
        app.services_table.list.is_scrolled(),
    )
}
