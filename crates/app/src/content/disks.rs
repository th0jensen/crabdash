use capitalize::Capitalize;
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::{
    app::Crabdash,
    components::{
        common::{button, lucide_icon},
        scroll_list,
    },
};

use super::shared::placeholder_card;
use services::{
    Disk, DiskNode,
    disks::{DiskAction, Disks},
};

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

    div()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(999.0))
        .bg(if healthy {
            rgb(0x193D2A)
        } else {
            rgb(0x47232B)
        })
        .text_xs()
        .text_color(if healthy {
            rgb(0x30D158)
        } else {
            rgb(0xFF453A)
        })
        .child(normalized.capitalize())
}

fn stats_chip(label: &str, value: String) -> Div {
    div()
        .h(px(34.0))
        .px(px(12.0))
        .py(px(7.0))
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x2F2F31))
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

fn disk_meta(disk: &Disk) -> String {
    [disk.size.clone(), disk.detail.clone()]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" • ")
}

fn node_meta(node: &DiskNode) -> Option<String> {
    let text = [node.size.clone(), node.detail.clone()]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" • ");

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

fn node_row(
    node: &DiskNode,
    ancestors: &[bool],
    is_last: bool,
    cx: &mut Context<Crabdash>,
    pending: bool,
    machine_index: usize,
) -> AnyElement {
    let tree_offset = ((ancestors.len() + 1) as f32 * TREE_COL_WIDTH) + 10.0;

    let action_button = node.is_mountable().then(|| {
        let action = if node.is_mounted() {
            DiskAction::Unmount
        } else {
            DiskAction::Mount
        };
        // Synthesize a temporary Disk so we can reuse disk_action_button
        let proxy = Disk {
            id: node.id.clone().unwrap_or_default(),
            name: node.name.clone(),
            size: node.size.clone(),
            status: node.status.clone().unwrap_or_default(),
            detail: node.detail.clone(),
            nodes: Vec::new(),
        };
        disk_action_button(cx, &proxy, action, pending, machine_index)
    });

    div()
        .relative()
        .w_full()
        .pb(px(TREE_ROW_GAP))
        .child(
            div()
                .pl(px(tree_offset))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .flex_col()
                        .gap(px(3.0))
                        .child(div().text_sm().text_color(white()).child(node.name.clone()))
                        .when_some(node_meta(node), |this, meta| {
                            this.child(div().text_xs().text_color(rgb(0x8E8E93)).child(meta))
                        }),
                )
                .when_some(action_button, |this, btn| this.child(btn)),
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

fn collect_rows(
    rows: &mut Vec<AnyElement>,
    nodes: &[DiskNode],
    ancestors: &[bool],
    cx: &mut Context<Crabdash>,
    pending_ids: &std::collections::HashSet<String>,
    machine_index: usize,
) {
    for (index, node) in nodes.iter().enumerate() {
        let is_last = index + 1 == nodes.len();
        let pending = node
            .id
            .as_deref()
            .is_some_and(|id| pending_ids.contains(id));
        rows.push(node_row(node, ancestors, is_last, cx, pending, machine_index));

        let mut next = ancestors.to_vec();
        next.push(!is_last);
        collect_rows(rows, &node.nodes, &next, cx, pending_ids, machine_index);
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

fn disk_action_button(
    cx: &mut Context<Crabdash>,
    disk: &Disk,
    action: DiskAction,
    pending: bool,
    machine_index: usize,
) -> AnyElement {
    let id = disk.id.clone();
    let button_id = SharedString::from(format!(
        "{}-disk-{}",
        match action {
            DiskAction::Mount => "mount",
            DiskAction::Unmount => "unmount",
        },
        id
    ));
    let icon = match action {
        DiskAction::Mount => lucide_icons::Icon::CirclePlay,
        DiskAction::Unmount => lucide_icons::Icon::CircleStop,
    };
    let color = match action {
        DiskAction::Mount => rgb(0x30D158),
        DiskAction::Unmount => rgb(0xFF453A),
    };

    button(button_id, icon, Option::<&str>::None, false)
        .text_color(color)
        .when(pending, |d| d.cursor_default())
        .when(!pending, |d| {
            d.on_click(cx.listener(move |this, _, _, cx| {
                this.pending_disk_actions.insert(id.clone(), action);
                cx.notify();

                let spawn_id = id.clone();
                let remove_id = id.clone();
                let mut machine = this.selected_machine().background_clone();

                cx.spawn(move |this: gpui::WeakEntity<Crabdash>, cx: &mut gpui::AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        let result = cx
                            .background_spawn(async move {
                                match action {
                                    DiskAction::Mount => machine.mount_disk(&spawn_id),
                                    DiskAction::Unmount => machine.unmount_disk(&spawn_id),
                                }
                            })
                            .await;

                        this.update(&mut cx, move |this, cx| {
                            this.pending_disk_actions.remove(&remove_id);
                            match result {
                                Ok(()) => {
                                    let mut fresh = this.selected_machine().background_clone();
                                    if let Ok(disks) = fresh.list_disks() {
                                        if let Some(m) =
                                            this.machine_store.machines.get_mut(machine_index)
                                        {
                                            m.services.disks = disks;
                                            m.services.disks_error = None;
                                        }
                                    }
                                    this.clear_status_message();
                                }
                                Err(err) => {
                                    this.set_status_error(format!(
                                        "Disk {} failed: {err}",
                                        match action {
                                            DiskAction::Mount => "mount",
                                            DiskAction::Unmount => "unmount",
                                        }
                                    ));
                                }
                            }
                            cx.notify();
                        })
                        .ok();
                    }
                })
                .detach();
            }))
        })
        .into_any_element()
}

fn disk_row(disk: &Disk, app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let machine_index = app.selected_machine;
    let disk_key = format!("{}:{}", machine_index, disk.id);
    let has_nodes = !disk.nodes.is_empty();
    let expanded = app.expanded_disk_rows.contains(&disk_key);
    let pending = app.pending_disk_actions.contains_key(&disk.id);
    let status = disk.status.to_ascii_lowercase();
    let is_mounted = status == "mounted";
    let is_swap = status == "swap";
    let pending_ids: std::collections::HashSet<String> =
        app.pending_disk_actions.keys().cloned().collect();
    let mut rows = Vec::new();

    if has_nodes && expanded {
        collect_rows(&mut rows, &disk.nodes, &[], cx, &pending_ids, machine_index);
    }

    div()
        .w_full()
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x2F2F31))
        .rounded(px(8.0))
        .px(px(14.0))
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .gap(px(12.0))
                        .child(tree_toggle_button(cx, &disk_key, has_nodes, expanded))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(div().text_sm().text_color(white()).child(disk.name.clone()))
                                .when(!disk_meta(disk).is_empty(), |this| {
                                    this.child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x8E8E93))
                                            .child(disk_meta(disk)),
                                    )
                                })
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0x636366))
                                        .child(format!("ID: {}", disk.id)),
                                ),
                        ),
                )
                .child(
                    div()
                        .h_full()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .when(!is_swap, |this| {
                            let action = if is_mounted {
                                DiskAction::Unmount
                            } else {
                                DiskAction::Mount
                            };
                            this.child(disk_action_button(cx, disk, action, pending, machine_index))
                        })
                        .child(status_badge(&disk).w(px(80.0)).text_center()),
                ),
        )
        .when(expanded && !rows.is_empty(), |this| {
            this.child(div().h(px(1.0)).bg(rgb(0x2F2F31)))
                .child(div().flex().flex_col().children(rows))
        })
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let machine = app.selected_machine();
    let disks = machine.services.disks.clone();

    let healthy = disks.iter().filter(|disk| disk.is_healthy()).count();

    scroll_list::render(
        "disks-scroll",
        &app.disks_scroll_handle,
        (!disks.is_empty()).then(|| {
            div()
                .flex()
                .gap(px(8.0))
                .child(stats_chip("Total", disks.len().to_string()))
                .child(stats_chip("Healthy", healthy.to_string()))
                .into_any_element()
        }),
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when(disks.is_empty(), |this| {
                this.child(placeholder_card(
                    "No Disks Found",
                    "There was an error when fetching disks.",
                ))
            })
            .when(!disks.is_empty(), |this| {
                this.children(disks.iter().map(|disk| disk_row(disk, app, cx)))
            }),
        cx,
    )
}
