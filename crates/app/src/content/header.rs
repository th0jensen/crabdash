//! Pane tabs reorder within their strip and drag into content to split or join.
mod drag;
mod scroll;

use crate::app::{Crabdash, MainTab};
use crate::components::{
    common::{control_tooltip, lucide_icon},
    style,
};
use crate::features::workspaces::model::{Drop, Tab};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct DraggedTab {
    pub(super) tab: MainTab,
    source_pane: u32,
    source_index: usize,
    owner: EntityId,
    workspace: Uuid,
    revision: Rc<Cell<Option<u64>>>,
}

impl DraggedTab {
    fn source(&self) -> drag::Source {
        drag::Source {
            workspace: self.workspace,
            pane: self.source_pane,
            index: self.source_index,
            tab: self.tab.into(),
        }
    }

    pub(super) fn validate(&self, app: &mut Crabdash, cx: &mut Context<Crabdash>) -> bool {
        app.sync_workspace_store(cx);
        let valid = self.owner == cx.entity_id()
            && self.source().accepts(
                self.revision.get(),
                app.workspaces.store.active,
                app.workspaces.revision(),
                app.workspaces.layout(),
            );
        if !valid && app.workspaces.drag_target.take().is_some() {
            cx.notify();
        }
        valid
    }
}

pub(super) fn valid_hover(owner: &WeakEntity<Crabdash>, drag: &DraggedTab, cx: &mut App) -> bool {
    owner
        .update(cx, |app, cx| drag.validate(app, cx))
        .is_ok_and(|valid| valid)
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(rems(style::BAR / 16.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(rems(4.0 / 16.0))
            .bg(rgb(style::SURFACE_HOVER))
            .text_size(rems(style::TEXT / 16.0))
            .text_color(rgb(style::TEXT_SELECTED))
            .shadow_md()
            .child(tab_icon(self.tab))
            .child(self.tab.label())
    }
}

fn tab_icon(tab: MainTab) -> Div {
    if tab == MainTab::Docker {
        crate::features::docker::brand::icon(style::ICON)
    } else {
        div()
            .size(rems(style::ICON / 16.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(lucide_icon(tab.icon(), style::ICON))
    }
}

fn text_width(app: &Crabdash, text: &str, size: f32, window: &Window) -> Pixels {
    let family = if app.preferences.interface_font.is_empty() {
        SharedString::from(".SystemUIFont")
    } else {
        app.preferences.interface_font.clone().into()
    };
    window
        .text_system()
        .shape_line(
            text.to_owned().into(),
            window.rem_size() * (size / 16.0),
            &[TextRun {
                len: text.len(),
                font: font(family),
                color: rgb(style::TEXT_PRIMARY).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
        .width
}

fn tab_width(app: &Crabdash, window: &Window) -> Pixels {
    let scale = window.rem_size() / 16.0;
    Tab::ALL
        .iter()
        .fold(scale * app.preferences.tab_width, |width, tab| {
            let tab: MainTab = (*tab).into();
            let title = text_width(app, tab.label(), style::TEXT, window);
            let hint = text_width(app, tab.shortcut(), style::META, window);
            let margin = shortcut_reserve(hint, scale);
            width.max(title + scale * (style::ICON + 4.0) + margin * 2.0)
        })
}

fn shortcut_reserve(hint: Pixels, scale: Pixels) -> Pixels {
    hint + scale * 8.0
}

fn reorder(
    app: &mut Crabdash,
    drag: &DraggedTab,
    pane: u32,
    target: Option<MainTab>,
    cx: &mut Context<Crabdash>,
) {
    if drag.validate(app, cx)
        && let Some((tabs, _)) = app.workspaces.layout().pane(pane)
    {
        let source = tabs.iter().position(|tab| *tab == drag.tab.into());
        let target = target.and_then(|target| tabs.iter().position(|tab| *tab == target.into()));
        let boundary = target.map_or(tabs.len(), |target| {
            target + usize::from(source.is_some_and(|source| source < target))
        });
        app.drop_workspace_tab(drag.tab, pane, Drop::Tab(boundary), cx);
    }
    cx.stop_propagation();
}

fn tab_button(
    app: &Crabdash,
    pane: u32,
    index: usize,
    tab: MainTab,
    active: bool,
    hints: bool,
    width: Pixels,
    title_width: Pixels,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let owner = cx.entity().downgrade();
    let hover_owner = owner.clone();
    let shortcut = || {
        div()
            .flex_none()
            .text_size(rems(style::META / 16.0))
            .child(tab.shortcut())
    };
    div()
        .id(SharedString::from(format!(
            "pane-{pane}-tab-{}",
            tab.label()
        )))
        .h_full()
        .min_w_0()
        .w(width)
        .flex_none()
        .px(rems(8.0 / 16.0))
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .text_size(rems(style::TEXT / 16.0))
        .line_height(relative(1.0))
        .whitespace_nowrap()
        .text_color(rgb(if active && app.workspaces.layout().focused == pane {
            style::TEXT_SELECTED
        } else if active {
            style::TEXT_PRIMARY
        } else {
            style::TEXT_MUTED
        }))
        .border_r_1()
        .border_color(rgb(style::BORDER))
        .bg(rgb(if active { 0x181818 } else { 0x202020 }))
        .when(!active, |this| this.border_b_1())
        .cursor_pointer()
        .hover(move |this| {
            this.bg(rgb(if active {
                0x181818
            } else {
                style::SURFACE_HOVER
            }))
            .text_color(rgb(style::TEXT_SELECTED))
        })
        .drag_over::<DraggedTab>(move |this, drag, _, cx| {
            if !valid_hover(&hover_owner, drag, cx) {
                return this;
            }
            let this = this
                .bg(rgb(0x383B3D))
                .border_0()
                .border_color(rgb(style::TAB_INDICATOR));
            if drag.source_pane != pane || index < drag.source_index {
                this.border_l_2()
            } else if index > drag.source_index {
                this.border_r_2()
            } else {
                this
            }
        })
        .on_drop(cx.listener(move |app, drag: &DraggedTab, _, cx| {
            // Our model uses boundaries in the original strip. Zed drops
            // before a target to the left and after a target to the right.
            reorder(app, drag, pane, Some(tab), cx);
        }))
        .when(!cx.has_active_drag(), |this| {
            // GPUI tooltip tasks retain their original bounds. Removing the
            // builder cancels a pending tooltip before a drag moves this tab.
            this.tooltip(move |_, cx| {
                control_tooltip(format!("{} · drag to reorder or split", tab.label()), cx)
            })
        })
        // Center the icon and title as one compact group. Reserve shortcut
        // space on both sides so holding Alt never shifts the group.
        .child(
            div()
                .flex_none()
                .h_full()
                .flex()
                .items_center()
                .gap(rems(4.0 / 16.0))
                .child(tab_icon(tab))
                .child(
                    div()
                        .w(title_width)
                        .min_w_0()
                        .text_ellipsis()
                        .overflow_hidden()
                        .child(tab.label()),
                ),
        )
        .child(
            div()
                .absolute()
                .right(rems(4.0 / 16.0))
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .text_color(rgb(style::TEXT_MUTED))
                .child(shortcut().opacity(if hints { 1.0 } else { 0.0 })),
        )
        .on_click(cx.listener(move |app, _, _, cx| app.select_pane_tab(pane, tab, cx)))
        .on_drag(
            DraggedTab {
                tab,
                source_pane: pane,
                source_index: index,
                owner: cx.entity_id(),
                workspace: app.workspaces.store.active,
                revision: Rc::new(Cell::new(None)),
            },
            move |drag, _, _, cx| {
                drag.revision.set(None);
                owner
                    .update(cx, |app, cx| {
                        app.sync_workspace_store(cx);
                        if drag.owner == cx.entity_id() {
                            drag.revision.set(drag.source().stamp(
                                app.workspaces.store.active,
                                app.workspaces.revision(),
                                app.workspaces.layout(),
                            ));
                        }
                        app.workspaces.drag_target = None;
                        cx.notify();
                    })
                    .ok();
                cx.new(|_| drag.clone())
            },
        )
}

pub(super) fn render(
    app: &Crabdash,
    pane: u32,
    tabs: &[Tab],
    active: Tab,
    window: &mut Window,
    viewport: Pixels,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let hints =
        app.preferences.always_show_shortcuts || window.modifiers().alt || app.open_menu.is_some();
    let width = tab_width(app, window);
    let scroll = scroll::handle(app, pane, tabs, active, width, viewport, window, cx);
    let hover_owner = cx.entity().downgrade();
    div()
        .id(SharedString::from(format!("pane-{pane}-tab-strip")))
        .h(rems(style::BAR / 16.0))
        .flex_none()
        .bg(rgb(0x202020))
        .flex()
        .items_center()
        .overflow_x_scroll()
        .track_scroll(&scroll)
        .on_drag_move(
            cx.listener(|app, event: &DragMoveEvent<DraggedTab>, _, cx| {
                let drag = event.drag(cx).clone();
                if !drag.validate(app, cx) {
                    return;
                }
                if event.bounds.contains(&event.event.position)
                    && app.workspaces.drag_target.take().is_some()
                {
                    cx.notify();
                }
            }),
        )
        .children(tabs.iter().enumerate().map(|(index, tab)| {
            let hint = text_width(app, MainTab::from(*tab).shortcut(), style::META, window);
            let scale = window.rem_size() / 16.0;
            let title_width = text_width(app, MainTab::from(*tab).label(), style::TEXT, window)
                .min(
                    (width - scale * (style::ICON + 4.0) - shortcut_reserve(hint, scale) * 2.0)
                        .max(px(0.0)),
                );
            tab_button(
                app,
                pane,
                index,
                (*tab).into(),
                *tab == active,
                hints,
                width,
                title_width,
                cx,
            )
        }))
        .child(
            div()
                .id(SharedString::from(format!("pane-{pane}-tab-strip-append")))
                .flex_1()
                .min_w(rems(24.0 / 16.0))
                .h_full()
                .border_b_1()
                .border_color(rgb(style::BORDER))
                .child("")
                .drag_over::<DraggedTab>(move |this, drag, _, cx| {
                    if valid_hover(&hover_owner, drag, cx) {
                        this.bg(rgb(0x383B3D))
                    } else {
                        this
                    }
                })
                .on_drop(cx.listener(move |app, drag: &DraggedTab, _, cx| {
                    reorder(app, drag, pane, None, cx);
                })),
        )
}
