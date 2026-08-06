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

pub(crate) const DEFAULT_SIDEBAR_WIDTH: f32 = 240.0;
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
    let connection_active = machine.connected();
    let icon = machine_icon(machine.kind);
    let name_color = if selected {
        rgb(0xF0F0F0)
    } else {
        rgb(0xC8C8C8)
    };
    let meta_color = if selected {
        rgb(0xA0A0A0)
    } else {
        rgb(0x858585)
    };
    let bg = if selected {
        rgb(0x2A2D2E)
    } else {
        rgb(0x1B1B1B)
    };
    let icon_bg = if selected {
        rgb(0x383838)
    } else {
        rgb(0x242424)
    };
    let dot = if connection_active {
        rgb(0x30D158)
    } else {
        rgb(0xFF453A)
    };
    let credentials_key = machine
        .remote
        .as_ref()
        .filter(|remote| remote.auth.is_some())
        .map(|remote| format!("com.thojensen.crabdash.ssh.{}@{}", remote.user, remote.host));

    div()
        .id(SharedString::from(format!("machine-{}", machine.id)))
        .w_full()
        .h(px(52.0))
        .px(px(10.0))
        .mx(px(4.0))
        .bg(bg)
        .rounded(px(5.0))
        .flex()
        .items_center()
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2D2E)))
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
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(28.0))
                                .h(px(28.0))
                                .rounded(px(4.0))
                                .bg(icon_bg)
                                .text_color(meta_color)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(lucide_icon(icon, 14.0)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(1.0))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(name_color)
                                        .child(machine.system_info.machine_name.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(meta_color)
                                        .child(machine.system_info.os_version.clone()),
                                ),
                        ),
                )
                .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(dot)),
        )
        .on_click(cx.listener(move |_this, _, window, cx| {
            window.activate_window();
            let credentials_key = credentials_key.clone();
            cx.spawn_in(window, async move |this: WeakEntity<Crabdash>, cx| {
                let credentials = if let Some(key) = credentials_key {
                    match cx.update(|_, app| app.read_credentials(&key)) {
                        Ok(credentials) => credentials.await.ok().flatten(),
                        Err(_) => None,
                    }
                } else {
                    None
                };

                this.update_in(cx, |this, window, cx| {
                    if let Some((_, bytes)) = credentials
                        && let Some(remote) = this
                            .machine_store
                            .machines
                            .get_mut(index)
                            .and_then(|machine| machine.remote.as_mut())
                        && let Some(auth) = remote.auth.as_mut()
                    {
                        auth.apply_secret(String::from_utf8_lossy(&bytes).into());
                    }

                    this.selected_machine = index;
                    this.refresh_services(cx);
                    if this.quake_terminal_open {
                        this.open_quake_terminal(window, cx);
                    } else {
                        window.focus(&this.focus_handle);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }))
}

fn add_machine_item(cx: &mut Context<Crabdash>) -> impl IntoElement {
    let bg = rgb(0x1B1B1B);
    let meta_color = rgb(0x858585);

    div()
        .id("open-add-machine-modal")
        .w_full()
        .h(px(38.0))
        .px(px(10.0))
        .mx(px(4.0))
        .bg(bg)
        .rounded(px(5.0))
        .flex()
        .items_center()
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2D2E)))
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
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(20.0))
                                .h(px(20.0))
                                .text_color(meta_color)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(lucide_icon(Icon::Plus, 12.0)),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(meta_color)
                                .child("Add machine"),
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
                        menu_app_refresh.update(cx, |app, cx| app.refresh_services(cx))
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
        .bg(rgb(0x1B1B1B))
        .border_r_1()
        .border_color(rgb(0x2B2B2B))
        .flex()
        .flex_col()
        .child(
            div()
                .id("machine-list-scroll")
                .flex_1()
                .overflow_y_scroll()
                .child(
                    div()
                        .h(px(34.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x777777))
                        .child("MACHINES")
                        .child(app.machine_store.machines.len().to_string()),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
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
