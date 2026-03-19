use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::Crabdash;
use crate::components::{
    common::{lucide_icon, machine_icon},
    context_menu::ContextMenu,
    right_click_menu::right_click_menu,
};
use machines::machine::Machine;

pub(crate) const DEFAULT_SIDEBAR_WIDTH: f32 = 250.0;
const MIN_SIDEBAR_WIDTH: f32 = 180.0;
const MAX_SIDEBAR_WIDTH: f32 = 420.0;
const SIDEBAR_RESIZE_HANDLE_WIDTH: f32 = 8.0;

#[derive(Clone)]
pub(crate) struct DraggedSidebarResize;

pub(crate) fn clamp_width(width: Pixels) -> Pixels {
    width.max(px(MIN_SIDEBAR_WIDTH)).min(px(MAX_SIDEBAR_WIDTH))
}

fn machine_item(
    machine: &Machine,
    index: usize,
    selected: bool,
    cx: &mut Context<Crabdash>,
) -> impl IntoElement {
    let connection_active = machine.has_active_connection();
    let icon = machine_icon(machine.kind);
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
    let border = rgb(0x2F2F31);
    let dot = if connection_active {
        rgb(0x30D158)
    } else {
        rgb(0xFF453A)
    };

    div()
        .id(SharedString::from(format!("machine-{}", machine.id)))
        .w_full()
        .h(px(58.0))
        .px(px(10.0))
        .bg(bg)
        .border_b_1()
        .border_color(border)
        .flex()
        .items_center()
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2A2C)))
        .child(
            div()
                .w_full()
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
                            .filter(|rc| rc.auth.is_some())
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
                                    if let Some(auth) = rc.auth.as_mut() {
                                        auth.apply_secret(String::from_utf8_lossy(&bytes).into());
                                    }
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

fn add_machine_item(cx: &mut Context<Crabdash>) -> impl IntoElement {
    let bg = rgb(0x1C1C1E);
    let border = rgb(0x2F2F31);
    let meta_color = rgb(0x8E8E93);

    div()
        .id("open-add-machine-modal")
        .w_full()
        .h(px(58.0))
        .px(px(10.0))
        .bg(bg)
        .border_t_1()
        .border_color(border)
        .flex()
        .items_center()
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2A2C)))
        .child(
            div()
                .w_full()
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
                                .text_color(meta_color)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(lucide_icon(Icon::Plus, 16.0)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(meta_color)
                                .child("Add New Machine"),
                        ),
                ),
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_add_machine_modal(window, cx);
        }))
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let app_entity = cx.entity();
    let machine_entries: Vec<_> = app
        .machine_store
        .machines
        .iter()
        .enumerate()
        .map(|(index, machine)| {
            let row =
                machine_item(machine, index, app.selected_machine == index, cx).into_any_element();
            let machine_uuid = machine.uuid;

            let menu_app = app_entity.clone();
            let is_localhost = machine.id == "localhost";
            right_click_menu(SharedString::from(format!(
                "machine-context-menu-{}",
                machine.id
            )))
            .trigger(move |_, _, _| row)
            .menu(move |window, cx| {
                let menu_app = menu_app.clone();
                ContextMenu::build(window, cx, move |menu, _, _| {
                    let menu_app_refresh = menu_app.clone();
                    let menu_app_delete = menu_app.clone();
                    let menu = menu.entry("Refresh", Icon::RefreshCw, None, move |_, cx| {
                        menu_app_refresh.update(cx, |app, _| app.refresh_services())
                    });
                    if is_localhost {
                        menu
                    } else {
                        menu.destructive_entry(
                            "Delete",
                            Icon::X,
                            Some(rgb(0xBA3C3C)),
                            move |_, cx| {
                                menu_app_delete
                                    .update(cx, |app, cx| app.delete_machine(machine_uuid, cx))
                            },
                        )
                    }
                })
            })
            .into_any_element()
        })
        .collect();

    div()
        .relative()
        .w(app.sidebar_width)
        .h_full()
        .flex_shrink_0()
        .bg(rgb(0x1C1C1E))
        .border_r_1()
        .border_color(rgb(0x2F2F31))
        .flex()
        .flex_col()
        .child(
            div()
                .id("machine-list-scroll")
                .flex_1()
                .overflow_y_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .children(machine_entries)
                        .child(add_machine_item(cx)),
                ),
        )
        .child(
            div()
                .id("sidebar-resize-handle")
                .absolute()
                .right(-px(SIDEBAR_RESIZE_HANDLE_WIDTH / 2.0))
                .top(px(0.0))
                .h_full()
                .w(px(SIDEBAR_RESIZE_HANDLE_WIDTH))
                .cursor_col_resize()
                .on_drag(DraggedSidebarResize, |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.new(|_| gpui::Empty)
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|app, event: &MouseUpEvent, _, cx| {
                        if event.click_count == 2 {
                            app.sidebar_width = px(DEFAULT_SIDEBAR_WIDTH);
                            cx.notify();
                            cx.stop_propagation();
                        }
                    }),
                )
                .occlude(),
        )
}
