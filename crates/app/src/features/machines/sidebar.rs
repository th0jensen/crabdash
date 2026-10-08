use super::logos;
pub(crate) mod palette;
// Keep the restored sidebar presentation independent of the newer dashboard chrome.
mod style {
    pub(super) const TEXT: f32 = 13.0;
    pub(super) const META: f32 = 12.0;
    pub(super) const ICON: f32 = 14.0;
    pub(super) const BAR: f32 = 36.0;
    pub(super) const RADIUS: f32 = 4.0;
    pub(super) const TEXT_PRIMARY: u32 = 0xD0D0D0;
    pub(super) const TEXT_SELECTED: u32 = 0xF2F2F2;
    pub(super) const TEXT_MUTED: u32 = 0x969696;
    pub(super) const ACCENT: u32 = 0xA9CEFF;
    pub(super) const SELECTED_BG: u32 = 0x252F3D;
    pub(super) const SELECTED_BORDER: u32 = 0x3C506A;
    pub(super) const SURFACE: u32 = 0x1E1E1E;
    pub(super) const SURFACE_HOVER: u32 = 0x282828;
    pub(super) const BORDER: u32 = 0x353535;
    pub(super) const CONTROL_SELECTED_BORDER: u32 = 0x555555;
    pub(super) const SUCCESS: u32 = 0x59C69A;
    pub(super) const CARD_RADIUS: f32 = 7.0;
}
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
                let name = machine.display_name();
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
    selected: bool,
    accent: Option<u32>,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let machine_uuid = machine.uuid;
    let connection_active = machine.connected();
    let is_remote = machine.remote.is_some();
    let name = machine.display_name().to_owned();
    let connection_label = if is_remote { "SSH" } else { "Local" };
    let metadata = format!("{} · {connection_label}", logos::platform_label(machine));
    let name_color = if selected {
        rgb(style::TEXT_SELECTED)
    } else {
        rgb(style::TEXT_PRIMARY)
    };
    let meta_color = rgb(if cfg!(target_os = "macos") && selected {
        style::TEXT_PRIMARY
    } else {
        style::TEXT_MUTED
    });
    // Keep the restored row geometry and dark surfaces. An optional desktop
    // accent changes only the selected row's existing colour treatment.
    let tint = |strength: u32, fallback| {
        accent.map_or(rgb(fallback), |accent| {
            rgb(palette::BACKGROUND).blend(rgba((accent << 8) | strength))
        })
    };
    let selected_bg = tint(38, style::SELECTED_BG);
    let selected_hover = tint(56, 0x2B3849);
    let bg = if selected {
        selected_bg
    } else {
        rgb(palette::BACKGROUND)
    };
    let border = if selected {
        tint(100, style::SELECTED_BORDER)
    } else {
        rgb(palette::BACKGROUND)
    };
    let icon_bg = if selected {
        tint(61, 0x304158)
    } else {
        rgb(style::SURFACE_HOVER)
    };
    let icon_color = if selected {
        rgb(accent.unwrap_or(style::ACCENT))
    } else {
        rgb(style::TEXT_MUTED)
    };
    let dot = if connection_active {
        rgb(style::SUCCESS)
    } else {
        rgb(style::TEXT_MUTED)
    };

    div()
        .id(SharedString::from(format!("machine-{}", machine.uuid)))
        .h(gpui::rems(60.0 / 16.0))
        .px(px(10.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .rounded(px(style::CARD_RADIUS))
        .flex()
        .items_center()
        .gap(px(10.0))
        .cursor_pointer()
        .hover(move |style| {
            style.bg(if selected {
                selected_hover
            } else {
                rgb(style::SURFACE_HOVER)
            })
        })
        .child(
            div()
                .relative()
                .size(gpui::rems(32.0 / 16.0))
                .flex_none()
                .rounded(px(style::CARD_RADIUS))
                .bg(icon_bg)
                .text_color(icon_color)
                .flex()
                .items_center()
                .justify_center()
                .child(logos::render(machine))
                .child(
                    div()
                        .absolute()
                        .right(-px(2.0))
                        .bottom(-px(2.0))
                        .size(px(9.0))
                        .rounded_full()
                        .border_2()
                        .border_color(bg)
                        .bg(dot),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    clipped_text(name)
                        .w_full()
                        .text_size(gpui::rems(style::TEXT / 16.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(name_color),
                )
                .child(
                    clipped_text(metadata)
                        .w_full()
                        .text_size(gpui::rems(style::META / 16.0))
                        .text_color(meta_color),
                ),
        )
        .on_click(cx.listener(move |app, _, window, cx| {
            app.select_machine(machine_uuid, window, cx);
        }))
}

fn add_machine_item(cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .id("open-add-machine-modal")
        .h(gpui::rems(style::BAR / 16.0))
        .w_full()
        .px(px(10.0))
        .bg(rgb(style::SURFACE))
        .border_1()
        .border_color(rgb(style::BORDER))
        .rounded(px(style::CARD_RADIUS))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .text_color(rgb(style::TEXT_PRIMARY))
        .text_size(gpui::rems(style::TEXT / 16.0))
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(rgb(style::SURFACE_HOVER))
                .border_color(rgb(style::CONTROL_SELECTED_BORDER))
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
        .child("Add machine")
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_add_machine_modal(window, cx);
        }))
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let app_entity = cx.entity();
    let accent = crate::desktop::appearance::system_accent(app.preferences.use_system_accent, cx);
    let machine_entries: Vec<_> = app
        .machine_store
        .machines
        .iter()
        .enumerate()
        .map(|(index, machine)| {
            let row = machine_item(machine, app.selected_machine == index, accent, cx);
            let machine_uuid = machine.uuid;

            let tooltip_app = app_entity.clone();
            let menu_app = app_entity.clone();
            let is_localhost = machine.id == "localhost";
            right_click_menu(SharedString::from(format!(
                "machine-context-menu-{}",
                machine.uuid
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
                    let menu_app_rename = menu_app.clone();
                    let menu = menu
                        .entry("Refresh", Icon::RefreshCw, None, move |_, cx| {
                            menu_app_refresh
                                .update(cx, |app, cx| app.refresh_machine(machine_uuid, cx))
                        })
                        .entry("Rename…", Icon::Pencil, None, move |window, cx| {
                            menu_app_rename.update(cx, |app, cx| {
                                app.open_machine_rename(machine_uuid, window, cx)
                            })
                        });
                    if is_localhost {
                        menu
                    } else {
                        menu.destructive_entry(
                            "Delete",
                            Icon::X,
                            Some(rgb(0xBA3C3C)),
                            move |window, cx| {
                                menu_app_delete.update(cx, |app, cx| {
                                    app.delete_machine(machine_uuid, window, cx)
                                })
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
        .bg(rgb(palette::BACKGROUND))
        .border_r_1()
        .border_color(rgb(0x2B2B2B))
        .flex()
        .flex_col()
        .child(
            div()
                .h(gpui::rems(style::BAR / 16.0))
                .flex_none()
                .px(px(14.0))
                .flex()
                .items_center()
                .justify_between()
                .text_size(gpui::rems(style::META / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(7.0))
                        .child(lucide_icon(Icon::Network, 13.0))
                        .child("Machines"),
                )
                .child(
                    div()
                        .px(px(6.0))
                        .rounded(px(style::RADIUS))
                        .bg(rgb(style::SURFACE_HOVER))
                        .text_color(rgb(style::TEXT_MUTED))
                        .child(app.machine_store.machines.len().to_string()),
                ),
        )
        .child(
            div()
                .id("machine-list-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(8.0))
                .py(px(6.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .children(machine_entries),
                ),
        )
        .child(
            div()
                .flex_none()
                .p(px(10.0))
                .border_t_1()
                .border_color(rgb(style::BORDER))
                .child(add_machine_item(cx)),
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
