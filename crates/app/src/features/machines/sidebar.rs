use super::logos;
use crate::components::style;
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::Crabdash;
use crate::components::{
    common::{clipped_text, control_tooltip, lucide_icon, tooltip_text},
    context_menu::ContextMenu,
    right_click_menu::right_click_menu,
};
use machines::machine::Machine;
use uuid::Uuid;

const MIN_SIDEBAR_WIDTH: f32 = 180.0;
const MAX_SIDEBAR_WIDTH: f32 = 420.0;
const SIDEBAR_RESIZE_HANDLE_WIDTH: f32 = 8.0;

#[derive(Clone)]
pub(crate) struct DraggedSidebarResize;

pub(crate) fn clamp_width(width: Pixels) -> Pixels {
    width.max(px(MIN_SIDEBAR_WIDTH)).min(px(MAX_SIDEBAR_WIDTH))
}

// Observe the machine store so an open tooltip reflects reconnects immediately.
struct MachineTooltip {
    app: Entity<Crabdash>,
    machine_uuid: Uuid,
    _changes: Subscription,
}

impl Render for MachineTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let label = app
            .machine_store
            .machines
            .iter()
            .find(|machine| machine.uuid == self.machine_uuid)
            .map(|machine| {
                let status = if machine.connected() {
                    "Connected"
                } else {
                    "Disconnected"
                };
                let endpoint = machine.remote.as_ref().map_or_else(
                    || "This machine".to_owned(),
                    |remote| format!("{}@{}", remote.user, remote.host),
                );
                let name = machine.system_info.machine_name.trim();
                let name = if name.is_empty() { &machine.id } else { name };
                let release = machine
                    .system_info
                    .distribution
                    .as_ref()
                    .map(|distribution| format!("{}\n", distribution.pretty_name))
                    .unwrap_or_default();
                format!(
                    "{name} · {status}\n{endpoint}\n{release}{}",
                    machine.system_info.os_version.trim()
                )
            })
            .unwrap_or_else(|| "Machine removed".to_owned());
        tooltip_text(label)
    }
}

fn machine_item(
    machine: &Machine,
    index: usize,
    selected: bool,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let connection_active = machine.connected();
    let is_remote = machine.remote.is_some();
    let name = if machine.system_info.machine_name.trim().is_empty() {
        machine.id.clone()
    } else {
        machine.system_info.machine_name.trim().to_owned()
    };
    let connection_label = if is_remote { "SSH" } else { "Local" };
    let metadata = format!("{} · {connection_label}", logos::platform_label(machine));
    let name_color = if selected {
        rgb(style::TEXT_SELECTED)
    } else {
        rgb(style::TEXT_PRIMARY)
    };
    let meta_color = rgb(style::TEXT_MUTED);
    let bg = if selected {
        rgb(style::SELECTED_BG)
    } else {
        rgb(style::CHROME)
    };
    let icon_color = if selected {
        rgb(style::TEXT_PRIMARY)
    } else {
        rgb(style::TEXT_MUTED)
    };
    let dot = if connection_active {
        rgb(style::SUCCESS)
    } else {
        rgb(style::TEXT_MUTED)
    };
    let credentials_key = machine
        .remote
        .as_ref()
        .filter(|remote| remote.auth.is_some())
        .map(|remote| format!("com.thojensen.crabdash.ssh.{}@{}", remote.user, remote.host));

    div()
        .id(SharedString::from(format!("machine-{}", machine.id)))
        .h(gpui::rems(40.0 / 16.0))
        .px(rems(8.0 / 16.0))
        .bg(bg)
        .flex()
        .items_center()
        .gap(rems(8.0 / 16.0))
        .cursor_pointer()
        .hover(move |style| {
            style.bg(if selected {
                rgb(style::SELECTED_BG)
            } else {
                rgb(style::SURFACE_HOVER)
            })
        })
        .child(
            div()
                .size(gpui::rems(20.0 / 16.0))
                .flex_none()
                .text_color(icon_color)
                .flex()
                .items_center()
                .justify_center()
                .child(logos::render(machine)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(rems(1.0 / 16.0))
                .line_height(relative(1.15))
                .child(
                    clipped_text(name)
                        .w_full()
                        .text_size(gpui::rems(style::TEXT / 16.0))
                        .text_color(name_color),
                )
                .child(
                    clipped_text(metadata)
                        .w_full()
                        .text_size(gpui::rems(style::META / 16.0))
                        .text_color(meta_color),
                ),
        )
        .child(
            div()
                .size(rems(5.0 / 16.0))
                .flex_none()
                .rounded_full()
                .bg(dot),
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

fn add_machine_button(cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .id("open-add-machine-modal")
        .size(rems(style::CHROME_CONTROL / 16.0))
        .flex_none()
        .rounded(px(style::RADIUS))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(style::TEXT_MUTED))
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(rgb(style::SURFACE_HOVER))
                .text_color(rgb(style::TEXT_SELECTED))
        })
        .tooltip(|_, cx| {
            control_tooltip(
                format!(
                    "Add machine ({})",
                    crate::desktop::menus::shortcut("⌘N", "Ctrl+N")
                ),
                cx,
            )
        })
        .child(lucide_icon(Icon::Plus, style::ICON))
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
            let row = machine_item(machine, index, app.selected_machine == index, cx);
            let machine_uuid = machine.uuid;

            let tooltip_app = app_entity.clone();
            let menu_app = app_entity.clone();
            let is_localhost = machine.id == "localhost";
            right_click_menu(SharedString::from(format!(
                "machine-context-menu-{}",
                machine.id
            )))
            .trigger(move |menu_open, _, _| {
                row.when(!menu_open, |row| {
                    row.tooltip(move |_, cx| {
                        cx.new(|cx| MachineTooltip {
                            app: tooltip_app.clone(),
                            machine_uuid,
                            _changes: cx.observe(&tooltip_app, |_, _, cx| cx.notify()),
                        })
                        .into()
                    })
                })
            })
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
        .bg(rgb(style::CHROME))
        .border_r_1()
        .border_color(rgb(style::BORDER))
        .flex()
        .flex_col()
        .child(
            div()
                .h(gpui::rems(style::BAR / 16.0))
                .flex_none()
                .px(rems(8.0 / 16.0))
                .flex()
                .items_center()
                .justify_between()
                .text_size(gpui::rems(style::TEXT / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rems(6.0 / 16.0))
                        .child("Machines")
                        .child(
                            div()
                                .text_size(rems(style::META / 16.0))
                                .child(app.machine_store.machines.len().to_string()),
                        ),
                )
                .child(add_machine_button(cx)),
        )
        .child(
            div()
                .id("machine-list-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(rems(4.0 / 16.0))
                .py(rems(4.0 / 16.0))
                .child(div().flex().flex_col().children(machine_entries)),
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
                .on_drag(DraggedSidebarResize, move |_, _, window, cx| {
                    // Apply the first drag position before the next move event.
                    app_entity.update(cx, |app, cx| {
                        app.set_sidebar_width(window.mouse_position().x, cx);
                    });
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
                            app.set_sidebar_width(px(app.preferences.sidebar_width), cx);
                            cx.stop_propagation();
                        }
                    }),
                )
                .occlude(),
        )
}
