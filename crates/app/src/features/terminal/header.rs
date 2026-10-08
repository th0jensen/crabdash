//! One pane header combines session tabs and the drawer actions.
use super::docking::DraggedSession;
use crate::layout::geometry::reveal_offset;
use crate::{
    app::Crabdash,
    components::{
        common::{chrome_icon_button, clipped_text, control_tooltip, lucide_icon},
        style,
    },
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use uuid::Uuid;

fn new_session(cx: &mut Context<Crabdash>) -> Stateful<Div> {
    chrome_icon_button("new-terminal-session", Icon::Plus)
        .tooltip(|_, cx| {
            control_tooltip(
                format!(
                    "New terminal · {}",
                    crate::desktop::menus::shortcut("⌘T", "Ctrl+Shift+T")
                ),
                cx,
            )
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|app, _, window, cx| app.new_quake_terminal(window, cx)))
}

fn hide(cx: &mut Context<Crabdash>) -> Stateful<Div> {
    chrome_icon_button("close-quake-terminal", Icon::X)
        .tooltip(|_, cx| control_tooltip("Hide terminal", cx))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|app, _, window, cx| app.set_quake_terminal_open(false, window, cx)))
}

fn action_slot(width: Pixels) -> Div {
    div()
        .w(width)
        .h_full()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .border_b_1()
        .border_color(crate::components::style::rgb(style::BORDER))
        .occlude()
}

#[derive(PartialEq)]
struct Geometry {
    tabs: Vec<Uuid>,
    active: Uuid,
    widths: Vec<Pixels>,
    viewport: Pixels,
}

#[derive(Default)]
struct ScrollState {
    scroll: ScrollHandle,
    geometry: Option<Geometry>,
}

fn text_width(app: &Crabdash, title: &str, window: &Window) -> Pixels {
    let family = if app.preferences.interface_font.is_empty() {
        SharedString::from(".SystemUIFont")
    } else {
        app.preferences.interface_font.clone().into()
    };
    window
        .text_system()
        .shape_line(
            title.to_owned().into(),
            window.rem_size() * (style::TEXT / 16.0),
            &[TextRun {
                len: title.len(),
                font: font(family),
                color: crate::components::style::rgb(style::TEXT_PRIMARY).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
        .width
}

fn tab_width(measured: Pixels, maximum: Pixels, viewport: Pixels) -> Pixels {
    measured.min(maximum).min(viewport.max(px(1.0)))
}

fn strip_width(widths: &[Pixels], viewport: Pixels, actions: bool) -> Pixels {
    let viewport = viewport.max(px(0.0));
    if actions {
        // The following sibling is Plus: stay next to the last tab when all
        // tabs fit, and at the available strip edge when they overflow.
        widths
            .iter()
            .copied()
            .fold(px(0.0), |sum, width| sum + width)
            .min(viewport)
    } else {
        viewport
    }
}

pub(super) fn tabs(
    app: &Crabdash,
    pane: u32,
    tabs: &[Uuid],
    active: Uuid,
    viewport: Pixels,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let machine = app.selected_machine().uuid;
    let Some(drawer) = app.quake_terminals.get(&machine) else {
        return div().id("empty-terminal-tabs");
    };
    let owner = cx.entity().downgrade();
    let role = super::panes::header_role(&drawer.model.layout.root, pane);
    let action_width = window.rem_size() * ((style::CHROME_CONTROL + 8.0) / 16.0);
    let viewport = (viewport
        - if role.actions {
            action_width * 2.0
        } else {
            px(0.0)
        })
    .max(px(0.0));
    let widths: Vec<_> = tabs
        .iter()
        .map(|id| {
            let title = drawer
                .sessions
                .get(id)
                .map(|session| {
                    let full_title = session.terminal.title();
                    let automatic = super::title::automatic(
                        session.program.as_deref(),
                        &full_title,
                        app.selected_machine().remote.is_none(),
                    );
                    super::rename::visible_title(session.custom_name.as_deref(), &automatic)
                })
                .unwrap_or_default();
            tab_width(
                text_width(app, &title, window)
                    + window.rem_size() * ((16.0 + 4.0 + style::CHROME_CONTROL) / 16.0)
                    + px(1.0),
                window.rem_size() * (app.preferences.tab_width / 16.0),
                viewport,
            )
        })
        .collect();
    let viewport = strip_width(&widths, viewport, role.actions);
    let geometry = Geometry {
        tabs: tabs.to_vec(),
        active,
        widths: widths.clone(),
        viewport,
    };
    let state = window.use_keyed_state(
        SharedString::from(format!("terminal-tab-scroll-{}-{pane}", drawer.model.scope)),
        cx,
        |_, _| ScrollState::default(),
    );
    let dragging = cx.has_active_drag();
    let scroll = state.update(cx, |state, _| {
        if !dragging && state.geometry.as_ref() != Some(&geometry) {
            if let Some(index) = tabs.iter().position(|tab| *tab == active) {
                let current = state.scroll.offset().x;
                let offset = reveal_offset(current, &widths, viewport, index);
                state.scroll.set_offset(point(offset, px(0.0)));
            }
            state.geometry = Some(geometry);
        }
        state.scroll.clone()
    });
    let accent = (drawer.model.layout.focused == pane)
        .then(|| crate::desktop::appearance::system_accent(app.preferences.use_system_accent, cx))
        .flatten();
    let end = tabs.len();
    let strip = div()
        .id(SharedString::from(format!(
            "terminal-tabs-{machine}-{pane}"
        )))
        .w(viewport)
        .min_w_0()
        .h_full()
        .flex_none()
        .flex()
        .items_center()
        .bg(crate::components::style::rgb(style::CHROME))
        .overflow_x_scroll()
        .track_scroll(&scroll)
        .on_drop(cx.listener(move |app, drag: &DraggedSession, window, cx| {
            if drag.valid(app, cx.entity_id()) {
                app.drop_terminal_tab(
                    machine,
                    drag.session,
                    pane,
                    crate::layout::Drop::Tab(end),
                    window,
                    cx,
                );
            }
            cx.stop_propagation();
        }))
        .children(tabs.iter().enumerate().filter_map(|(index, id)| {
            let session = drawer.sessions.get(id)?;
            let width = widths[index];
            let id = *id;
            let full_title = session.terminal.title();
            let automatic = super::title::automatic(
                session.program.as_deref(),
                &full_title,
                app.selected_machine().remote.is_none(),
            );
            let title = super::rename::visible_title(session.custom_name.as_deref(), &automatic);
            let editing = session.rename.is_some();
            let tooltip_title =
                super::rename::tooltip_title(session.custom_name.as_deref(), &full_title);
            let hint = format!(
                "{tooltip_title}\n{} · drag to reorder or split · double-click title to rename",
                session.endpoint
            );
            let source = DraggedSession::new(app, pane, index, id, cx.entity_id(), title.clone());
            let rename_title = title.clone();
            let start_owner = owner.clone();
            Some(
                div()
                    .id(SharedString::from(format!("terminal-tab-{id}")))
                    .h_full()
                    .w(width)
                    .flex_none()
                    .min_w_0()
                    .px(rems(8.0 / 16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .gap(rems(4.0 / 16.0))
                    .bg(crate::components::style::rgb(if id == active {
                        style::CONTENT
                    } else {
                        style::CHROME
                    }))
                    .text_size(rems(style::TEXT / 16.0))
                    .line_height(relative(1.0))
                    .text_color(crate::components::style::rgb(
                        if id == active && drawer.model.layout.focused == pane {
                            style::TEXT_SELECTED
                        } else if id == active {
                            style::TEXT_PRIMARY
                        } else {
                            style::TEXT_MUTED
                        },
                    ))
                    .border_r_1()
                    .border_color(crate::components::style::rgb(style::BORDER))
                    .cursor_pointer()
                    .hover(move |this| {
                        this.bg(crate::components::style::rgb(if id == active {
                            style::CONTENT
                        } else {
                            style::SURFACE_HOVER
                        }))
                        .text_color(crate::components::style::rgb(style::TEXT_SELECTED))
                    })
                    .when(id != active, |tab| tab.border_b_1())
                    .when(!editing && !cx.has_active_drag(), |tab| {
                        tab.tooltip(move |_, cx| control_tooltip(hint.clone(), cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(4.0))
                            .when_some(accent.filter(|_| id == active), |this, accent| {
                                this.text_color(rgb(accent))
                            })
                            .when_some(session.rename.as_ref(), |this, editor| {
                                this.child(super::rename::render(editor, cx))
                            })
                            .when(!editing, |this| {
                                this.child(
                                    clipped_text(title)
                                        .id(SharedString::from(format!("terminal-title-{id}")))
                                        .flex_shrink()
                                        .on_click(cx.listener(
                                            move |app, event: &ClickEvent, window, cx| {
                                                if event.click_count() == 2 {
                                                    app.begin_terminal_rename(
                                                        machine,
                                                        id,
                                                        &rename_title,
                                                        window,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }
                                            },
                                        )),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("close-terminal-tab-{id}")))
                            .size(rems(style::CHROME_CONTROL / 16.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(style::RADIUS))
                            .text_color(crate::components::style::rgb(style::TAB_CLOSE))
                            .cursor_pointer()
                            .hover(|this| {
                                this.bg(crate::components::style::rgb(style::SURFACE_HOVER))
                            })
                            .when(!cx.has_active_drag(), |this| {
                                this.tooltip(|_, cx| {
                                    control_tooltip(
                                        format!(
                                            "Close terminal session · {}",
                                            crate::desktop::menus::shortcut("⌘⇧W", "Ctrl+Shift+W")
                                        ),
                                        cx,
                                    )
                                })
                            })
                            .child(lucide_icon(Icon::X, style::ICON))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.close_terminal_session(machine, id, window, cx);
                                cx.stop_propagation();
                            })),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(!editing, |this| {
                        this.on_click(cx.listener(move |app, _, window, cx| {
                            app.select_terminal_session(machine, id, window, cx)
                        }))
                    })
                    .on_drop(cx.listener(move |app, drag: &DraggedSession, window, cx| {
                        if drag.valid(app, cx.entity_id()) {
                            let boundary =
                                index + usize::from(drag.pane == pane && drag.index < index);
                            app.drop_terminal_tab(
                                machine,
                                drag.session,
                                pane,
                                crate::layout::Drop::Tab(boundary),
                                window,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    }))
                    .when(!editing, |this| {
                        this.on_drag(source, move |drag, _, _, cx| {
                            start_owner
                                .update(cx, |app, cx| {
                                    if drag.valid(app, cx.entity_id()) {
                                        if let Some(drawer) = app.quake_terminals.get_mut(&machine)
                                        {
                                            drawer.model.drag_target = None;
                                        }
                                    }
                                })
                                .ok();
                            cx.new(|_| drag.clone())
                        })
                    }),
            )
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .border_b_1()
                .border_color(crate::components::style::rgb(style::BORDER)),
        );
    let row = div()
        .id(SharedString::from(format!(
            "terminal-pane-header-{machine}-{pane}"
        )))
        .w_full()
        .min_w_0()
        .h(rems(style::BAR / 16.0))
        .flex_none()
        .flex()
        .items_center()
        .overflow_hidden()
        .bg(crate::components::style::rgb(style::CHROME))
        .child(strip)
        .when(role.actions, |this| {
            this.child(action_slot(action_width).child(new_session(cx)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .border_b_1()
                        .border_color(crate::components::style::rgb(style::BORDER)),
                )
                .child(action_slot(action_width).child(hide(cx)))
        });
    if role.resize {
        super::height::resize_header(row, app, cx)
    } else {
        row
    }
}

#[cfg(test)]
mod tests {
    use super::{reveal_offset, strip_width, tab_width};
    use gpui::px;

    #[test]
    fn measured_tab_width_fits_the_available_header_viewport() {
        assert_eq!(tab_width(px(92.0), px(144.0), px(400.0)), px(92.0));
        assert_eq!(tab_width(px(240.0), px(144.0), px(400.0)), px(144.0));
        assert_eq!(tab_width(px(240.0), px(144.0), px(70.0)), px(70.0));
        assert_eq!(tab_width(px(92.0), px(144.0), px(0.0)), px(1.0));
    }

    #[test]
    fn plus_follows_the_last_tab_and_pins_to_the_strip_edge_on_overflow() {
        let available = px(400.0) - px(32.0) * 2.0;
        assert_eq!(
            strip_width(&[px(92.0), px(80.0)], available, true),
            px(172.0)
        );
        assert_eq!(
            strip_width(&[px(92.0), px(244.0)], available, true),
            available
        );
        assert_eq!(
            strip_width(&[px(92.0), px(300.0)], available, true),
            available
        );
        assert_eq!(strip_width(&[], available, true), px(0.0));
        assert_eq!(strip_width(&[px(92.0)], available, false), available);
        assert_eq!(strip_width(&[px(92.0)], px(-10.0), true), px(0.0));
    }

    #[test]
    fn variable_width_reveal_uses_each_title_after_selection_and_resize() {
        let widths = [px(92.0), px(80.0), px(110.0), px(86.0)];
        assert_eq!(reveal_offset(px(0.0), &widths, px(250.0), 2), px(-32.0));
        assert_eq!(reveal_offset(px(-32.0), &widths, px(200.0), 3), px(-168.0));
        assert_eq!(reveal_offset(px(-168.0), &widths, px(200.0), 0), px(0.0));
        // A visible selection keeps the user's manual scroll position.
        assert_eq!(reveal_offset(px(-32.0), &widths, px(400.0), 2), px(-32.0));
    }

    #[test]
    fn oversized_or_unavailable_tabs_keep_safe_reveal_offsets() {
        let widths = [px(92.0), px(300.0), px(110.0)];
        assert_eq!(reveal_offset(px(-92.0), &widths, px(400.0), 1), px(-92.0));
        assert_eq!(reveal_offset(px(0.0), &widths, px(200.0), 1), px(-92.0));
        assert_eq!(reveal_offset(px(-10.0), &widths, px(0.0), 2), px(-10.0));
        assert_eq!(reveal_offset(px(-10.0), &widths, px(200.0), 99), px(-10.0));
    }
}
