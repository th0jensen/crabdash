use crate::app::Crabdash;
use crate::components::{
    common::{clipped_text, control_tooltip, lucide_icon, surface_button},
    style,
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;

const INFO_LINE: f32 = 18.0;

pub(super) fn row_height(window: &Window) -> Pixels {
    window.rem_size() * ((style::CONTROL + 10.0) / 16.0)
}

fn wrapped_height(
    text: &str,
    width: Pixels,
    lines: usize,
    app: &Crabdash,
    window: &Window,
) -> Pixels {
    let size = window.rem_size() * (style::META / 16.0);
    let family = if app.preferences.interface_font.is_empty() {
        SharedString::from(".SystemUIFont")
    } else {
        app.preferences.interface_font.clone().into()
    };
    let count = match window.text_system().shape_text(
        text.to_owned().into(),
        size,
        &[TextRun {
            len: text.len(),
            font: font(family),
            color: rgb(style::TEXT_MUTED).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        Some(width.max(px(1.0))),
        Some(lines),
    ) {
        Ok(shaped) => shaped
            .iter()
            .map(|line| line.wrap_boundaries.len() + 1)
            .sum::<usize>()
            .max(1)
            .min(lines),
        Err(error) => {
            tracing::warn!(%error, "Unable to measure workspace popup text");
            lines
        }
    };
    window.rem_size() * (INFO_LINE / 16.0) * count as f32
}

pub(crate) fn button(app: &Crabdash, window: &Window, cx: &mut Context<Crabdash>) -> Stateful<Div> {
    let name = app.workspaces.store.current().name.clone();
    let control = window.rem_size() * (style::CHROME_CONTROL / 16.0);
    let popup_visible = app.workspaces.open
        && !app.preferences_open
        && !app.add_machine_modal_open
        && !app.docker_run_modal_open
        && app.docker_removal.is_none()
        && app.open_menu.is_none();
    div()
        .id("workspace-switcher")
        .size(rems(style::CHROME_CONTROL / 16.0))
        .flex_none()
        .relative()
        .rounded(px(style::RADIUS))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(style::TEXT_MUTED))
        .when(app.workspaces.open, |this| {
            this.bg(rgb(style::CONTROL_SELECTED_BG))
        })
        .cursor_pointer()
        .hover(|this| {
            this.bg(rgb(style::SURFACE_HOVER))
                .text_color(rgb(style::TEXT_SELECTED))
        })
        .tooltip(move |_, cx| control_tooltip(format!("Workspaces · {name}"), cx))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(lucide_icon(Icon::PanelsTopLeft, style::ICON))
        .on_click(cx.listener(|app, _, window, cx| {
            app.toggle_workspace_popup(window, cx);
        }))
        .when(popup_visible, |this| {
            this.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .size(px(0.0))
                    .child(deferred(
                        anchored()
                            .position_mode(AnchoredPositionMode::Local)
                            .anchor(Corner::TopRight)
                            .position(point(control, control + window.rem_size() * (6.0 / 16.0)))
                            .snap_to_window_with_margin(px(12.0))
                            .child(popup(app, window, cx)),
                    )),
            )
        })
}

#[cfg(target_os = "macos")]
pub(crate) fn native_popup(
    app: &Crabdash,
    window: &Window,
    cx: &mut Context<Crabdash>,
) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .right_0()
        .size(px(0.0))
        .child(deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .anchor(Corner::TopRight)
                .position(point(window.viewport_size().width - px(12.0), px(6.0)))
                .snap_to_window_with_margin(px(12.0))
                .child(popup(app, window, cx)),
        ))
}

fn popup(app: &Crabdash, window: &Window, cx: &mut Context<Crabdash>) -> Stateful<Div> {
    let scale = f32::from(window.rem_size()) / 16.0;
    let viewport = window.viewport_size();
    let width = px(324.0 * scale).min((viewport.width - px(24.0)).max(px(0.0)));
    let top = px((if crate::desktop::shell::is_native(app) {
        6.0
    } else {
        style::TITLE_BAR + 6.0
    }) * scale);
    let max_height = px(440.0 * scale).min((viewport.height - top - px(12.0)).max(px(0.0)));
    let read_only = app.workspaces.read_only;
    let error = app
        .workspaces
        .rename_error
        .as_ref()
        .or(app.workspaces.error.as_ref())
        .or(app.workspaces.save_error.as_ref());
    let footer = if read_only {
        "The saved layout file is preserved. Reset saved layouts to enable saving."
    } else {
        "Changes to the active workspace are saved automatically."
    };
    let text_width = (width - px(36.0 * scale) - px(2.0)).max(px(1.0));
    let footer_height = wrapped_height(footer, text_width, 4, app, window);
    let error_height = error.map_or(px(0.0), |error| {
        wrapped_height(error, text_width, 6, app, window)
    });
    let header_height = px((style::CONTROL + 6.0) * scale);
    let save_height = px((style::CONTROL + 8.0) * scale + 1.0);
    let row_height = row_height(window);
    let gaps = 3 + usize::from(error.is_some()) + usize::from(read_only);
    let chrome_height = px(20.0 * scale + 2.0)
        + header_height
        + save_height
        + footer_height
        + error_height
        + px(gaps as f32 * 8.0 * scale)
        + if read_only {
            px(style::CONTROL * scale)
        } else {
            px(0.0)
        };
    let height =
        (chrome_height + row_height * app.workspaces.store.workspaces.len() as f32).min(max_height);
    let list_height = (height - chrome_height).max(px(0.0));
    let rows = app
        .workspaces
        .store
        .workspaces
        .iter()
        .map(|workspace| {
            let id = workspace.id;
            let active = id == app.workspaces.store.active;
            let editing = app
                .workspaces
                .rename
                .as_ref()
                .is_some_and(|draft| draft.id == id);
            div()
                .id(SharedString::from(format!("workspace-{id}")))
                .w_full()
                .min_w_0()
                .h(row_height)
                .flex_none()
                .px(rems(8.0 / 16.0))
                .py(rems(5.0 / 16.0))
                .flex()
                .items_center()
                .gap(rems(6.0 / 16.0))
                .rounded(px(style::RADIUS))
                .when(active, |this| this.bg(rgb(style::CONTROL_SELECTED_BG)))
                .when(!editing, |this| {
                    this.cursor_pointer()
                        .hover(|this| this.bg(rgb(style::SURFACE_HOVER)))
                })
                .child(lucide_icon(
                    if active {
                        Icon::Check
                    } else {
                        Icon::PanelsTopLeft
                    },
                    style::ICON,
                ))
                .when(editing, |this| {
                    this.child(div().flex_1().min_w_0().child(app.workspaces.name.clone()))
                })
                .when(!editing, |this| {
                    this.child(clipped_text(workspace.name.clone()).flex_1())
                })
                .when(editing, |this| {
                    this.child(
                        surface_button(
                            SharedString::from(format!("save-workspace-name-{id}")),
                            Some(Icon::Check),
                            None,
                        )
                        .when(read_only, disabled)
                        .when(!read_only, |this| {
                            this.on_click(cx.listener(|app, _, window, cx| {
                                app.finish_workspace_name(window, cx);
                                cx.stop_propagation();
                            }))
                        }),
                    )
                })
                .when(!editing, |this| {
                    this.child(
                        surface_button(
                            SharedString::from(format!("rename-workspace-{id}")),
                            Some(Icon::Pencil),
                            None,
                        )
                        .tooltip(|_, cx| control_tooltip("Rename workspace", cx))
                        .when(read_only, disabled)
                        .when(!read_only, |this| {
                            this.on_click(cx.listener(move |app, _, window, cx| {
                                app.rename_workspace(id, window, cx);
                                cx.stop_propagation();
                            }))
                        }),
                    )
                })
                .when(
                    !editing && app.workspaces.store.workspaces.len() > 1,
                    |this| {
                        this.child(
                            surface_button(
                                SharedString::from(format!("remove-workspace-{id}")),
                                Some(Icon::Trash2),
                                None,
                            )
                            .tooltip(|_, cx| control_tooltip("Delete saved workspace", cx))
                            .when(read_only, disabled)
                            .when(!read_only, |this| {
                                this.on_click(cx.listener(move |app, _, window, cx| {
                                    app.remove_workspace(id, window, cx);
                                    cx.stop_propagation();
                                }))
                            }),
                        )
                    },
                )
                .on_click(cx.listener(move |app, _, window, cx| {
                    if editing {
                        return;
                    }
                    app.restore_workspace(id, window, cx);
                }))
        })
        .collect::<Vec<_>>();

    div()
        .id("workspace-popup")
        .w(width)
        // Both dimensions must be definite: an auto-height popup containing a
        // flexing list causes costly recursive intrinsic layout in Taffy.
        .h(height)
        .overflow_hidden()
        .flex()
        .flex_col()
        .gap(rems(8.0 / 16.0))
        .p(rems(10.0 / 16.0))
        .bg(rgb(style::SURFACE))
        .border_1()
        .border_color(rgb(style::BORDER))
        .rounded(px(style::CARD_RADIUS))
        .shadow_lg()
        .text_size(rems(style::TEXT / 16.0))
        .text_color(rgb(style::TEXT_PRIMARY))
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex_none()
                .h(header_height)
                .px(rems(8.0 / 16.0))
                .py(rems(3.0 / 16.0))
                .flex()
                .items_center()
                .justify_between()
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Workspaces"))
                .child(
                    surface_button("workspace-close", Some(Icon::X), None).on_click(cx.listener(
                        |app, _, window, cx| {
                            app.workspaces.open = false;
                            app.workspaces.rename = None;
                            app.workspaces.rename_error = None;
                            app.workspaces.pending_rename_focus = true;
                            app.apply_workspace_runtime(window, cx);
                            cx.notify();
                        },
                    )),
                ),
        )
        .child(
            div()
                .id("workspace-list")
                .h(list_height)
                .flex_none()
                .w_full()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&app.workspaces.scroll)
                .children(rows),
        )
        .when_some(error, |this, error| {
            this.child(
                div()
                    .flex_none()
                    .h(error_height)
                    .px(rems(8.0 / 16.0))
                    .text_size(rems(style::META / 16.0))
                    .line_height(rems(INFO_LINE / 16.0))
                    .line_clamp(6)
                    .text_color(rgb(style::DANGER))
                    .child(error.clone()),
            )
        })
        .when(app.workspaces.read_only, |this| {
            this.child(
                surface_button(
                    "workspace-recover",
                    Some(Icon::RotateCcw),
                    Some("Reset saved layouts"),
                )
                .on_click(cx.listener(|app, _, _, cx| app.recover_workspaces(cx))),
            )
        })
        .child(
            div()
                .border_t_1()
                .border_color(rgb(style::BORDER))
                .flex_none()
                .h(save_height)
                .pt(rems(8.0 / 16.0))
                .flex()
                .items_center()
                .justify_end()
                .child(
                    surface_button("workspace-save-as", Some(Icon::Plus), Some("Save as…"))
                        .when(read_only, disabled)
                        .when(!read_only, |this| {
                            this.on_click(
                                cx.listener(|app, _, window, cx| app.save_workspace_as(window, cx)),
                            )
                        }),
                ),
        )
        .child(
            div()
                .flex_none()
                .h(footer_height)
                .px(rems(8.0 / 16.0))
                .text_size(rems(style::META / 16.0))
                .line_height(rems(INFO_LINE / 16.0))
                .line_clamp(4)
                .text_color(rgb(style::TEXT_MUTED))
                .child(footer),
        )
}

fn disabled(control: Stateful<Div>) -> Stateful<Div> {
    control
        .cursor_default()
        .text_color(rgb(style::TEXT_MUTED))
        .opacity(0.5)
        .hover(|this| {
            this.bg(rgb(style::SURFACE))
                .text_color(rgb(style::TEXT_MUTED))
        })
        .on_click(|_, _, cx| cx.stop_propagation())
}
