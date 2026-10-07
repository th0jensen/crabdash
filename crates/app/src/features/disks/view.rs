use super::table::{Column, Filter};
use crate::components::style;
use capitalize::Capitalize;
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::{
    app::Crabdash,
    components::{common::lucide_icon, scroll_list},
};

use crate::components::table::{
    STATUS_WIDTH, clipped_text, filter_chip, fixed_column, placeholder_card, sort_heading,
    status_label, table_card, table_heading, table_row, toolbar,
};
use utils::disks::{Disk, DiskNode};

const TREE_LINE: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.38,
    a: 1.0,
};
const TREE_COL_WIDTH: f32 = 14.0;
const TREE_ELBOW_Y: f32 = 18.0;
const TREE_ROW_GAP: f32 = 10.0;

fn status_badge(disk: &Disk) -> Div {
    let normalized = disk.status.to_ascii_lowercase();
    let healthy = matches!(normalized.as_str(), "mounted" | "healthy" | "swap");
    let color = if healthy {
        rgb(style::SUCCESS)
    } else {
        rgb(style::TEXT_MUTED)
    };
    status_label(normalized.capitalize(), color)
}
fn stats_chip(
    id: &'static str,
    label: &str,
    count: usize,
    filter: Filter,
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    filter_chip(id, label, count, app.disks_table.filter == filter).on_click(cx.listener(
        move |this, _, _, cx| {
            this.disks_table.filter = filter;
            this.disks_scroll_handle.set_offset(point(px(0.0), px(0.0)));
            cx.notify();
        },
    ))
}
fn disk_meta(disk: &Disk) -> String {
    [disk.size.clone(), disk.detail.clone()]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn node_meta(node: &DiskNode) -> Option<String> {
    let text = [node.size.clone(), node.detail.clone()]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ");

    (!text.is_empty()).then_some(text)
}

fn guide_column(active: bool) -> Div {
    div()
        .relative()
        .w(px(TREE_COL_WIDTH))
        .h_full()
        .when(active, |this| {
            this.child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(-px(TREE_ROW_GAP / 2.0))
                    .bottom(-px(TREE_ROW_GAP / 2.0))
                    .w(px(1.0))
                    .bg(TREE_LINE),
            )
        })
}

fn branch_column(is_last: bool) -> Div {
    div()
        .relative()
        .w(px(TREE_COL_WIDTH))
        .h_full()
        .child(
            div()
                .absolute()
                .left(px(6.0))
                .top(-px(TREE_ROW_GAP / 2.0))
                .h(px(TREE_ELBOW_Y + (TREE_ROW_GAP / 2.0)))
                .w(px(1.0))
                .bg(TREE_LINE),
        )
        .when(!is_last, |this| {
            this.child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(TREE_ELBOW_Y))
                    .bottom(-px(TREE_ROW_GAP / 2.0))
                    .w(px(1.0))
                    .bg(TREE_LINE),
            )
        })
        .child(
            div()
                .absolute()
                .left(px(6.0))
                .top(px(TREE_ELBOW_Y))
                .w(px(8.0))
                .h(px(1.0))
                .bg(TREE_LINE),
        )
}

fn node_row(node: &DiskNode, ancestors: &[bool], is_last: bool) -> AnyElement {
    let tree_offset = ((ancestors.len() + 1) as f32 * TREE_COL_WIDTH) + 10.0;

    div()
        .relative()
        .w_full()
        .pb(px(TREE_ROW_GAP))
        .child(
            div()
                .pl(px(tree_offset))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    clipped_text(node.name.clone())
                        .w_full()
                        .text_size(rems(style::TEXT / 16.0))
                        .text_color(rgb(style::TEXT_PRIMARY)),
                )
                .when_some(node_meta(node), |this, meta| {
                    this.child(
                        clipped_text(meta)
                            .w_full()
                            .text_size(rems(style::META / 16.0))
                            .text_color(rgb(style::TEXT_MUTED)),
                    )
                }),
        )
        .child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .bottom(px(0.0))
                .flex()
                .children(ancestors.iter().copied().map(guide_column))
                .child(branch_column(is_last)),
        )
        .into_any_element()
}

fn collect_rows(rows: &mut Vec<AnyElement>, nodes: &[DiskNode], ancestors: &[bool]) {
    for (index, node) in nodes.iter().enumerate() {
        let is_last = index + 1 == nodes.len();
        rows.push(node_row(node, ancestors, is_last));

        let mut next = ancestors.to_vec();
        next.push(!is_last);
        collect_rows(rows, &node.nodes, &next);
    }
}

fn tree_toggle_button(
    cx: &mut Context<Crabdash>,
    disk_id: &str,
    has_nodes: bool,
    expanded: bool,
) -> AnyElement {
    let button = div()
        .id(SharedString::from(format!("disk-toggle-{disk_id}")))
        .h(px(24.0))
        .w(px(24.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .text_color(rgb(0xAEAEB2));

    if !has_nodes {
        return button.into_any_element();
    }

    let disk_id = disk_id.to_string();

    button
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x343437)))
        .child(lucide_icon(
            if expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            14.0,
        ))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_disk_row(&disk_id, cx);
        }))
        .into_any_element()
}

fn disk_row(disk: &Disk, app: &Crabdash, cx: &mut Context<Crabdash>, show_id: bool) -> Div {
    let disk_key = format!("{}:{}", app.selected_machine, disk.id);
    let has_nodes = !disk.nodes.is_empty();
    let expanded = app.expanded_disk_rows.contains(&disk_key);
    let mut rows = Vec::new();
    if has_nodes && expanded {
        collect_rows(&mut rows, &disk.nodes, &[]);
    }
    div()
        .w_full()
        .bg(rgb(style::SURFACE))
        .border_b_1()
        .border_color(rgb(style::BORDER))
        .py(px(10.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            table_row()
                .id(SharedString::from(format!("disk-row-{}", disk.id)))
                .hover(|s| s.bg(rgb(style::SURFACE_HOVER)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(tree_toggle_button(cx, &disk_key, has_nodes, expanded))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .child(
                                    clipped_text(disk.name.clone())
                                        .w_full()
                                        .text_size(rems(style::TEXT / 16.0))
                                        .text_color(rgb(style::TEXT_PRIMARY)),
                                )
                                .child(
                                    clipped_text(disk_meta(disk))
                                        .w_full()
                                        .text_size(rems(style::META / 16.0))
                                        .text_color(rgb(style::TEXT_MUTED)),
                                ),
                        ),
                )
                .when(show_id, |this| {
                    this.child(
                        fixed_column(160.0).child(
                            clipped_text(disk.id.clone())
                                .w_full()
                                .text_size(rems(style::META / 16.0))
                                .text_color(rgb(style::TEXT_MUTED)),
                        ),
                    )
                })
                .child(fixed_column(STATUS_WIDTH).child(status_badge(disk))),
        )
        .when(expanded && !rows.is_empty(), |this| {
            this.child(div().h(px(1.0)).bg(rgb(style::BORDER)))
                .child(div().px(px(12.0)).flex().flex_col().children(rows))
        })
}
fn table_header(show_id: bool, app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    table_heading()
        .child(
            div().flex_1().min_w_0().h_full().pl(px(32.0)).child(
                sort_heading(
                    "disks-sort-name",
                    "Device",
                    app.disks_table.sort.indicator(Column::Name),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.disks_table.sort.select(Column::Name);
                    this.disks_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                    cx.notify();
                })),
            ),
        )
        .when(show_id, |this| {
            this.child(
                fixed_column(160.0).h_full().child(
                    sort_heading(
                        "disks-sort-id",
                        "ID",
                        app.disks_table.sort.indicator(Column::Id),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.disks_table.sort.select(Column::Id);
                        this.disks_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                        cx.notify();
                    })),
                ),
            )
        })
        .child(
            fixed_column(STATUS_WIDTH).h_full().child(
                sort_heading(
                    "disks-sort-status",
                    "Status",
                    app.disks_table.sort.indicator(Column::Status),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.disks_table.sort.select(Column::Status);
                    this.disks_scroll_handle.set_offset(point(px(0.0), px(0.0)));
                    cx.notify();
                }))
                .justify_center(),
            ),
        )
}

pub fn render(app: &Crabdash, window: &Window, cx: &mut Context<Crabdash>) -> Div {
    let show_id = window.viewport_size().width
        - if app.sidebar_collapsed {
            px(0.0)
        } else {
            app.sidebar_width
        }
        >= px(560.0 * app.preferences.interface_font_size / 13.0);
    let machine = app.selected_machine();
    let disks = machine.services.disks.clone();
    let mounted = disks.iter().filter(|d| Filter::Mounted.matches(d)).count();
    let visible_disks = app
        .disks_table
        .visible(&disks, &app.disks_table.search.query(cx));

    scroll_list::render(
        "disks-scroll",
        &app.disks_scroll_handle,
        Some(
            toolbar(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(stats_chip(
                        "disks-filter-all",
                        "All",
                        disks.len(),
                        Filter::All,
                        app,
                        cx,
                    ))
                    .child(stats_chip(
                        "disks-filter-mounted",
                        "Mounted",
                        mounted,
                        Filter::Mounted,
                        app,
                        cx,
                    ))
                    .child(stats_chip(
                        "disks-filter-unmounted",
                        "Unmounted",
                        disks.len() - mounted,
                        Filter::Unmounted,
                        app,
                        cx,
                    )),
                &app.disks_table.search,
            )
            .into_any_element(),
        ),
        div()
            .flex()
            .flex_col()
            .when(disks.is_empty(), |this| {
                this.child(placeholder_card(
                    "No disks",
                    "No disks have been loaded for this machine yet.",
                ))
            })
            .when(!disks.is_empty() && visible_disks.is_empty(), |this| {
                this.child(placeholder_card(
                    "No matching disks",
                    "Try another status filter or clear the search field.",
                ))
            })
            .when(!visible_disks.is_empty(), |this| {
                this.child(
                    table_card().child(table_header(show_id, app, cx)).children(
                        visible_disks
                            .iter()
                            .map(|disk| disk_row(disk, app, cx, show_id)),
                    ),
                )
            }),
        cx,
    )
}
