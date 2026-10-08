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
use lucide_icons::Icon;
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
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // GPUI shares the active drag across windows. Its preview belongs to
        // the source window, just like this tab's drop targets.
        if window
            .root::<Crabdash>()
            .flatten()
            .is_none_or(|view| view.entity_id() != self.owner)
        {
            return div();
        }
        crate::components::drag_preview::render(self.tab.label(), tab_icon(self.tab, None))
    }
}

fn tab_icon(tab: MainTab, accent: Option<u32>) -> Div {
    if tab == MainTab::Docker {
        crate::features::docker::brand::icon_with_color(
            style::ICON,
            accent.unwrap_or(crate::components::style::color(style::TEXT_PRIMARY)),
        )
    } else {
        div()
            .size(rems(style::ICON / 16.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .when_some(accent, |this, accent| this.text_color(rgb(accent)))
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
                color: crate::components::style::rgb(style::TEXT_PRIMARY).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
        .width
}

fn tab_width(app: &Crabdash, tab: MainTab, window: &Window) -> Pixels {
    let scale = window.rem_size() / 16.0;
    // Both controls and the two compact gaps belong to this tab's title group.
    text_width(app, tab.label(), style::TEXT, window)
        + scale * (16.0 + style::ICON + 4.0 + 4.0 + 18.0)
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
    width: Pixels,
    title_width: Pixels,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let owner = cx.entity().downgrade();
    let hover_owner = owner.clone();
    let accent = (active && app.workspaces.layout().focused == pane)
        .then(|| crate::desktop::appearance::system_accent(app.preferences.use_system_accent, cx))
        .flatten();
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
        .flex()
        .items_center()
        .justify_center()
        .text_size(rems(style::TEXT / 16.0))
        .line_height(relative(1.0))
        .whitespace_nowrap()
        .text_color(crate::components::style::rgb(
            if active && app.workspaces.layout().focused == pane {
                style::TEXT_SELECTED
            } else if active {
                style::TEXT_PRIMARY
            } else {
                style::TEXT_MUTED
            },
        ))
        .border_r_1()
        .border_color(crate::components::style::rgb(style::BORDER))
        .bg(crate::components::style::rgb(if active {
            0x181818
        } else {
            0x202020
        }))
        .when(!active, |this| this.border_b_1())
        .cursor_pointer()
        .hover(move |this| {
            this.bg(crate::components::style::rgb(if active {
                0x181818
            } else {
                style::SURFACE_HOVER
            }))
            .text_color(crate::components::style::rgb(style::TEXT_SELECTED))
        })
        .drag_over::<DraggedTab>(move |this, drag, _, cx| {
            if !valid_hover(&hover_owner, drag, cx) {
                return this;
            }
            let this = this
                .bg(crate::components::style::rgb(0x383B3D))
                .border_0()
                .border_color(crate::components::style::rgb(style::TAB_INDICATOR));
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
        // The icon, measured title and close control form one compact group.
        .child(
            div()
                .flex_none()
                .h_full()
                .flex()
                .items_center()
                .gap(rems(4.0 / 16.0))
                .child(tab_icon(tab, accent))
                .child(
                    div()
                        .w(title_width)
                        .min_w_0()
                        .text_ellipsis()
                        .overflow_hidden()
                        .when_some(accent, |this, accent| this.text_color(rgb(accent)))
                        .child(tab.label()),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "pane-{pane}-close-{}",
                            tab.label()
                        )))
                        .size(rems(18.0 / 16.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .text_color(crate::components::style::rgb(style::TAB_CLOSE))
                        .opacity(0.65)
                        .hover(|this| {
                            this.opacity(1.0)
                                .bg(crate::components::style::rgb(style::SURFACE_HOVER))
                        })
                        .tooltip(move |_, cx| control_tooltip(format!("Close {}", tab.label()), cx))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.close_workspace_tab(pane, tab, cx);
                            cx.stop_propagation();
                        }))
                        .child(lucide_icon(Icon::X, 12.0)),
                ),
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
    let widths: Vec<_> = tabs
        .iter()
        .map(|tab| tab_width(app, (*tab).into(), window))
        .collect();
    let scroll = scroll::handle(app, pane, tabs, active, &widths, viewport, window, cx);
    let hover_owner = cx.entity().downgrade();
    div()
        .id(SharedString::from(format!("pane-{pane}-tab-strip")))
        .h(rems(style::BAR / 16.0))
        .flex_none()
        .bg(crate::components::style::rgb(0x202020))
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
            let width = widths[index];
            let title_width = text_width(app, MainTab::from(*tab).label(), style::TEXT, window);
            tab_button(
                app,
                pane,
                index,
                (*tab).into(),
                *tab == active,
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
                .border_color(crate::components::style::rgb(style::BORDER))
                .child("")
                .drag_over::<DraggedTab>(move |this, drag, _, cx| {
                    if valid_hover(&hover_owner, drag, cx) {
                        this.bg(crate::components::style::rgb(0x383B3D))
                    } else {
                        this
                    }
                })
                .on_drop(cx.listener(move |app, drag: &DraggedTab, _, cx| {
                    reorder(app, drag, pane, None, cx);
                })),
        )
}
