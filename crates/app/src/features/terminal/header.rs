//! One pane header combines session tabs and the drawer actions.
use super::docking::DraggedSession;
use crate::{
    app::Crabdash,
    components::{
        common::{clipped_text, control_tooltip, lucide_icon},
        style,
    },
    desktop::controls,
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use uuid::Uuid;

fn display_title(title: &str, endpoint: &str, reported_hostname: &str) -> String {
    let hostname = reported_hostname.trim();
    let reported_identity = endpoint
        .rsplit_once('@')
        .filter(|(user, _)| !user.is_empty() && !hostname.is_empty())
        .map(|(user, _)| format!("{user}@{hostname}"));
    let identities = [Some(endpoint), reported_identity.as_deref()];
    let title = title.trim();
    let remainder = identities
        .iter()
        .flatten()
        .filter(|identity| !identity.is_empty())
        .find_map(|identity| title.strip_prefix(*identity)?.strip_prefix(':'))
        .map_or(title, str::trim);
    if remainder.is_empty()
        || identities
            .into_iter()
            .flatten()
            .any(|identity| title == identity)
    {
        "Terminal".into()
    } else {
        remainder.to_owned()
    }
}

fn actions(cx: &mut Context<Crabdash>) -> AnyElement {
    controls::toolbar_group(
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(rems(4.0 / 16.0)),
        vec![
            controls::toolbar_icon_button("new-terminal-session", "New terminal", Icon::Plus)
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
                .on_click(cx.listener(|app, _, window, cx| app.new_quake_terminal(window, cx))),
            controls::toolbar_icon_button("close-quake-terminal", "Hide terminal", Icon::X)
                .tooltip(|_, cx| control_tooltip("Hide terminal", cx))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(
                    cx.listener(|app, _, window, cx| {
                        app.set_quake_terminal_open(false, window, cx)
                    }),
                ),
        ],
    )
}
#[derive(Default)]
struct ScrollState {
    scroll: ScrollHandle,
    geometry: Option<(Vec<Uuid>, Uuid, Pixels, Pixels)>,
}

fn tab_width(preferred: Pixels, viewport: Pixels) -> Pixels {
    preferred.min(viewport.max(px(1.0)))
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
    let action_width = if role.actions {
        controls::toolbar_group_width(
            2.0 * style::CHROME_CONTROL + 4.0,
            &[&[Icon::Plus], &[Icon::X]],
            app.preferences.liquid_glass,
            window,
            cx,
        ) + window.rem_size()
    } else {
        px(0.0)
    };
    let viewport = (viewport - action_width).max(px(0.0));
    let width = tab_width(
        window.rem_size() * (app.preferences.tab_width / 16.0),
        viewport,
    );
    let narrow = width < window.rem_size() * (120.0 / 16.0);
    let geometry = (tabs.to_vec(), active, width, viewport);
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
                let offset =
                    crate::layout::geometry::reveal_offset(current, width, viewport, index);
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
        .bg(rgb(style::CHROME))
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
            let id = *id;
            let full_title = session.terminal.title();
            let automatic = display_title(
                &full_title,
                &session.endpoint,
                &app.selected_machine().system_info.machine_name,
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
                    .pl(rems(
                        if narrow {
                            4.0
                        } else {
                            style::CHROME_CONTROL + 8.0
                        } / 16.0,
                    ))
                    .pr(rems((style::CHROME_CONTROL + 8.0) / 16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .bg(rgb(if id == active {
                        style::CONTENT
                    } else {
                        style::CHROME
                    }))
                    .text_size(rems(style::TEXT / 16.0))
                    .line_height(relative(1.0))
                    .text_color(rgb(
                        if id == active && drawer.model.layout.focused == pane {
                            style::TEXT_SELECTED
                        } else if id == active {
                            style::TEXT_PRIMARY
                        } else {
                            style::TEXT_MUTED
                        },
                    ))
                    .border_r_1()
                    .border_color(rgb(style::BORDER))
                    .cursor_pointer()
                    .hover(move |this| {
                        this.bg(rgb(if id == active {
                            style::CONTENT
                        } else {
                            style::SURFACE_HOVER
                        }))
                        .text_color(rgb(style::TEXT_SELECTED))
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
                            .child(lucide_icon(Icon::Terminal, style::ICON))
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
                            .absolute()
                            .right(rems(4.0 / 16.0))
                            .top_0()
                            .bottom_0()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .id(SharedString::from(format!("close-terminal-tab-{id}")))
                                    .size(rems(style::CHROME_CONTROL / 16.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(style::RADIUS))
                                    .cursor_pointer()
                                    .hover(|this| this.bg(rgb(style::SURFACE_HOVER)))
                                    .when(!cx.has_active_drag(), |this| {
                                        this.tooltip(|_, cx| {
                                            control_tooltip(
                                                format!(
                                                    "Close terminal session · {}",
                                                    crate::desktop::menus::shortcut(
                                                        "⌘⇧W",
                                                        "Ctrl+Shift+W"
                                                    )
                                                ),
                                                cx,
                                            )
                                        })
                                    })
                                    .child(lucide_icon(Icon::X, style::ICON))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        app.close_terminal_session(machine, id, window, cx);
                                        cx.stop_propagation();
                                    })),
                            ),
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
                .min_w(rems(24.0 / 16.0))
                .h_full()
                .border_b_1()
                .border_color(rgb(style::BORDER)),
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
        .bg(rgb(style::CHROME))
        .child(strip)
        .when(role.actions, |this| {
            this.child(
                div()
                    .w(action_width)
                    .h_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_b_1()
                    .border_color(rgb(style::BORDER))
                    .occlude()
                    .child(actions(cx)),
            )
        });
    if role.resize {
        super::height::resize_header(row, app, cx)
    } else {
        row
    }
}

#[cfg(test)]
mod tests {
    use super::{display_title, tab_width};
    use gpui::px;

    #[test]
    fn preference_tab_width_fits_the_available_header_viewport() {
        assert_eq!(tab_width(px(144.0), px(400.0)), px(144.0));
        assert_eq!(tab_width(px(240.0), px(70.0)), px(70.0));
        assert_eq!(tab_width(px(144.0), px(0.0)), px(1.0));
    }

    #[test]
    fn known_transport_and_reported_host_identities_are_removed() {
        assert_eq!(
            display_title("thomas@fedora:~", "thomas@127.0.0.1", "fedora"),
            "~"
        );
        assert_eq!(
            display_title("thomas@127.0.0.1:~", "thomas@127.0.0.1", "fedora"),
            "~"
        );
        assert_eq!(
            display_title("thomas@fedora: /a:b", "thomas@127.0.0.1", " fedora "),
            "/a:b"
        );
        assert_eq!(display_title("雪@家: /work", "雪@::1", "家"), "/work");
        assert_eq!(
            display_title("user@realm@fedora:~", "user@realm@::1", "fedora"),
            "~"
        );
        assert_eq!(display_title("thomas@::1:~", "thomas@::1", "fedora"), "~");
    }

    #[test]
    fn empty_titles_and_bare_known_identities_use_the_fallback() {
        for title in [
            "",
            "  ",
            "thomas@127.0.0.1",
            "thomas@fedora",
            "thomas@fedora: ",
        ] {
            assert_eq!(
                display_title(title, "thomas@127.0.0.1", "fedora"),
                "Terminal"
            );
        }
    }

    #[test]
    fn custom_titles_and_other_identities_are_preserved() {
        for title in [
            "editor: project",
            "other@fedora:~",
            "thomas@Fedora:~",
            "thomas@fedora-extra:~",
            "thomas@other:~",
            "note: thomas@fedora:~",
            "contact a@b",
        ] {
            assert_eq!(display_title(title, "thomas@127.0.0.1", "fedora"), title);
        }
        assert_eq!(
            display_title("thomas@fedora:~", "thomas@127.0.0.1", ""),
            "thomas@fedora:~"
        );
        assert_eq!(
            display_title("@fedora:~", "@127.0.0.1", "fedora"),
            "@fedora:~"
        );
        assert_eq!(display_title(": project", "", "fedora"), ": project");
    }
}
