use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::Crabdash;
use crate::components::common::{LucideIcon, button, lucide_icon, machine_icon};
use machines::machine::{Machine, MachineKind};

fn machine_item(
    machine: &Machine,
    index: usize,
    selected: bool,
    icon: LucideIcon,
    cx: &mut Context<Crabdash>,
) -> impl IntoElement {
    let name_color = if selected {
        rgb(0xFFFFFF)
    } else {
        rgb(0xD1D1D6)
    };
    let meta_color = if selected {
        rgb(0x0A84FF)
    } else {
        rgb(0x8E8E93)
    };
    let bg = if selected {
        rgb(0x2C2C2E)
    } else {
        rgb(0x1C1C1E)
    };
    let icon_bg = if selected {
        rgb(0x1F3656)
    } else {
        rgb(0x232326)
    };
    let border = rgb(0x3A3A3C);
    let dot = match machine.kind {
        MachineKind::MacOS => rgb(0x64D2FF),
        MachineKind::Linux => rgb(0x30D158),
        MachineKind::Unknown => rgb(0x000000),
    };

    div()
        .id(SharedString::from(format!("machine-{}", machine.id)))
        .w_full()
        .p(px(10.0))
        .bg(bg)
        .border_b_1()
        .border_color(border)
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2A2C)))
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            div()
                                .w(px(32.0))
                                .h(px(32.0))
                                .rounded(px(10.0))
                                .bg(icon_bg)
                                .text_color(meta_color)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(lucide_icon(icon, 16.0)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(name_color)
                                        .child(machine.system_info.machine_name.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(meta_color)
                                        .child(machine.system_info.os_version.clone()),
                                ),
                        ),
                )
                .child(div().w(px(8.0)).h(px(8.0)).rounded(px(999.0)).bg(dot)),
        )
        .on_click(cx.listener(move |_, _, _, cx| {
            cx.spawn(async move |this: WeakEntity<Crabdash>, cx| {
                let creds_key = this
                    .update(cx, |this, _| {
                        this.selected_machine = index;
                        this.selected_machine_mut()
                            .remote
                            .as_ref()
                            .filter(|rc| rc.password.is_empty())
                            .map(|rc| format!("com.thojensen.crabdash.ssh.{}@{}", rc.user, rc.host))
                    })
                    .ok()
                    .flatten();
                if let Some(key) = creds_key {
                    let creds = cx.update(|app| app.read_credentials(&key)).ok();
                    if let Some(creds_future) = creds {
                        if let Ok(Some((_, bytes))) = creds_future.await {
                            this.update(cx, |this, _| {
                                if let Some(rc) = this.selected_machine_mut().remote.as_mut() {
                                    rc.password = String::from_utf8_lossy(&bytes).to_string();
                                }
                            })
                            .ok();
                        }
                    }
                }
                this.update(cx, |this, cx| {
                    this.refresh_services();
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }))
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let machine_entries: Vec<_> = app
        .machine_store
        .machines
        .iter()
        .enumerate()
        .map(|(index, machine)| {
            machine_item(
                machine,
                index,
                app.selected_machine == index,
                machine_icon(machine.kind),
                cx,
            )
            .into_any_element()
        })
        .collect();

    div()
        .w(px(250.0))
        .h_full()
        .bg(rgb(0x1C1C1E))
        .border_r_1()
        .border_color(rgb(0x3A3A3C))
        .flex()
        .flex_col()
        .child(
            div()
                .id("machine-list-scroll")
                .flex_1()
                .overflow_y_scroll()
                .child(div().flex().flex_col().children(machine_entries)),
        )
        .child(
            div()
                .p(px(12.0))
                .border_t_1()
                .border_color(rgb(0x3A3A3C))
                .child(
                    button(
                        "open-add-machine-modal",
                        Icon::Plus,
                        Some("Add New Machine"),
                        false,
                    )
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_add_machine_modal(window, cx);
                    })),
                ),
        )
}
