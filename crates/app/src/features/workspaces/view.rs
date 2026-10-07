use crate::app::Crabdash;
use crate::components::{
    common::{control_tooltip, lucide_icon, surface_button},
    style,
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;

pub(crate) fn button(app: &Crabdash, cx: &mut Context<Crabdash>) -> Stateful<Div> {
    let name = app.workspaces.store.current().name.clone();
    div()
        .id("workspace-switcher")
        .size(rems(style::CONTROL / 16.0))
        .flex_none()
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
        .child(lucide_icon(Icon::LayoutGrid, style::ICON))
        .on_click(cx.listener(|app, _, window, cx| {
            app.sync_workspace_store(cx);
            app.workspaces.open = !app.workspaces.open;
            app.overlay_window = Some(window.window_handle().window_id());
            app.workspaces.rename = None;
            app.focus_handle.focus(window);
            cx.notify();
        }))
}

pub(crate) fn popup(app: &Crabdash, cx: &mut Context<Crabdash>) -> Stateful<Div> {
    let rows = app
        .workspaces
        .store
        .workspaces
        .iter()
        .map(|workspace| {
            let id = workspace.id;
            let active = id == app.workspaces.store.active;
            let editing = app.workspaces.rename == Some(id);
            div()
                .id(SharedString::from(format!("workspace-{id}")))
                .w_full()
                .min_h(rems(style::CONTROL / 16.0))
                .px(px(8.0))
                .py(px(5.0))
                .flex()
                .items_center()
                .gap(px(6.0))
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
                        Icon::LayoutGrid
                    },
                    style::ICON,
                ))
                .when(editing, |this| {
                    this.child(div().flex_1().min_w_0().child(app.workspaces.name.clone()))
                })
                .when(!editing, |this| {
                    this.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(workspace.name.clone()),
                    )
                })
                .when(editing, |this| {
                    this.child(
                        surface_button(
                            SharedString::from(format!("save-workspace-name-{id}")),
                            Some(Icon::Check),
                            None,
                        )
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.finish_workspace_name(window, cx);
                            cx.stop_propagation();
                        })),
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
                        .on_click(cx.listener(
                            move |app, _, window, cx| {
                                app.rename_workspace(id, window, cx);
                                cx.stop_propagation();
                            },
                        )),
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
                            .on_click(cx.listener(
                                move |app, _, window, cx| {
                                    app.remove_workspace(id, window, cx);
                                    cx.stop_propagation();
                                },
                            )),
                        )
                    },
                )
                .on_click(cx.listener(move |app, event: &ClickEvent, window, cx| {
                    if editing {
                        return;
                    }
                    if event.click_count() == 2 {
                        app.rename_workspace(id, window, cx);
                    } else {
                        app.restore_workspace(id, window, cx);
                    }
                }))
        })
        .collect::<Vec<_>>();

    div()
        .id("workspace-popup")
        .absolute()
        .right(px(92.0))
        .top(rems((style::TITLE_BAR + 6.0) / 16.0))
        .w(rems(324.0 / 16.0))
        .max_h(rems(440.0 / 16.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(10.0))
        .bg(rgb(style::SURFACE))
        .border_1()
        .border_color(rgb(style::BORDER))
        .rounded(px(style::CARD_RADIUS))
        .shadow_lg()
        .text_size(rems(style::TEXT / 16.0))
        .text_color(rgb(style::TEXT_PRIMARY))
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .capture_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
            match event.keystroke.key.as_str() {
                "enter" if app.workspaces.rename.is_some() => app.finish_workspace_name(window, cx),
                "escape" => {
                    if app.workspaces.rename.take().is_none() {
                        app.workspaces.open = false;
                    }
                    app.workspaces.error = None;
                    app.focus_handle.focus(window);
                    cx.notify();
                }
                _ => return,
            }
            cx.stop_propagation();
        }))
        .child(
            div()
                .px(px(8.0))
                .py(px(3.0))
                .flex()
                .items_center()
                .justify_between()
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Workspaces"))
                .child(
                    surface_button("workspace-close", Some(Icon::X), None).on_click(cx.listener(
                        |app, _, _, cx| {
                            app.workspaces.open = false;
                            cx.notify();
                        },
                    )),
                ),
        )
        .child(
            div()
                .id("workspace-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(rows),
        )
        .when_some(app.workspaces.error.as_ref(), |this, error| {
            this.child(
                div()
                    .px(px(8.0))
                    .text_size(rems(style::META / 16.0))
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
                .pt(px(8.0))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    surface_button(
                        "workspace-reset-grid",
                        Some(Icon::PanelsTopLeft),
                        Some("Reset grid"),
                    )
                    .on_click(cx.listener(|app, _, _, cx| app.reset_workspace_grid(cx))),
                )
                .child(
                    surface_button("workspace-save-as", Some(Icon::Plus), Some("Save as…"))
                        .on_click(
                            cx.listener(|app, _, window, cx| app.save_workspace_as(window, cx)),
                        ),
                ),
        )
        .child(
            div()
                .px(px(8.0))
                .text_size(rems(style::META / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child("Changes to the active workspace are saved automatically."),
        )
}
