use capitalize::Capitalize;
use gpui::prelude::*;
use gpui::*;

use crate::{app::Crabdash, components::scroll_list};

use super::shared::placeholder_card;
use services::{Disk, DiskNode};

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
                .child(div().text_sm().text_color(white()).child(node.name.clone()))
                .when_some(node_meta(node), |this, meta| {
                    this.child(div().text_xs().text_color(rgb(0x8E8E93)).child(meta))
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

fn disk_row(disk: &Disk) -> Div {
    let mut rows = Vec::new();
    collect_rows(&mut rows, &disk.nodes, &[]);

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
                .justify_between()
                .items_start()
                .gap(px(12.0))
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
                )
                .child(
                    div()
                        .h_full()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        // .child(if !container.is_running_status() {
                        //     action_button(cx, container, DockerAction::Start, actions_disabled)
                        // } else {
                        //     action_button(cx, container, DockerAction::Stop, actions_disabled)
                        // })
                        // .child(action_button(
                        //     cx,
                        //     container,
                        //     DockerAction::Restart,
                        //     actions_disabled,
                        // ))
                        .child(status_badge(&disk).w(px(80.0)).text_center()),
                ),
        )
        .when(!rows.is_empty(), |this| {
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
                this.children(disks.iter().map(disk_row))
            }),
        cx,
    )
}
