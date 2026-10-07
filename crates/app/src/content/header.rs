//! Flat tabs with insertion targets for reordering within the tab strip.
use crate::app::{Crabdash, MainTab};
use crate::components::{
    common::{clipped_text, control_tooltip, lucide_icon},
    style,
};
use gpui::{prelude::*, *};
use uuid::Uuid;

#[derive(Clone)]
struct DraggedTab {
    tab: MainTab,
    source_index: usize,
    owner: EntityId,
    workspace: Uuid,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(rems(style::BAR / 16.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(7.0))
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
        lucide_icon(tab.icon(), style::ICON)
    }
}

fn reorder(
    app: &mut Crabdash,
    drag: &DraggedTab,
    target: Option<MainTab>,
    cx: &mut Context<Crabdash>,
) {
    app.sync_workspace_store(cx);
    if drag.owner == cx.entity_id() && drag.workspace == app.workspaces.store.active {
        let tabs = &app.workspaces.layout().tabs;
        let source = tabs.iter().position(|tab| *tab == drag.tab.into());
        let target = target.and_then(|target| tabs.iter().position(|tab| *tab == target.into()));
        if let Some(source) = source {
            let boundary =
                target.map_or(tabs.len(), |target| target + usize::from(source < target));
            app.reorder_workspace_tab(drag.tab, boundary, cx);
        }
    }
    cx.stop_propagation();
}

fn tab_button(
    app: &Crabdash,
    index: usize,
    tab: MainTab,
    active: bool,
    hints: bool,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("tab-{}", tab.label())))
        .h_full()
        .min_w_0()
        .max_w(rems(app.preferences.tab_width / 16.0))
        .flex_none()
        .px(rems(4.0 / 16.0))
        .relative()
        .flex()
        .items_center()
        .gap(rems(4.0 / 16.0))
        .text_size(rems(style::TEXT / 16.0))
        .whitespace_nowrap()
        .text_color(rgb(if active {
            style::TEXT_SELECTED
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
        .drag_over::<DraggedTab>(move |this, drag, _, _| {
            let this = this
                .bg(rgb(0x383B3D))
                .border_0()
                .border_color(rgb(style::TAB_INDICATOR));
            if index < drag.source_index {
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
            reorder(app, drag, Some(tab), cx);
        }))
        .tooltip(move |_, cx| control_tooltip(format!("{} · drag to reorder", tab.label()), cx))
        .child(tab_icon(tab))
        .child(clipped_text(tab.label()))
        // Preserve intrinsic width and label position as Alt hints appear.
        .child(
            div()
                .flex_none()
                .opacity(if hints { 1.0 } else { 0.0 })
                .text_size(rems(style::META / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child(tab.shortcut()),
        )
        .on_click(cx.listener(move |app, _, _, cx| app.select_workspace_tab(tab, cx)))
        .on_drag(
            DraggedTab {
                tab,
                source_index: index,
                owner: cx.entity_id(),
                workspace: app.workspaces.store.active,
            },
            |drag, _, _, cx| cx.new(|_| drag.clone()),
        )
}

pub(super) fn render(
    app: &Crabdash,
    window: &Window,
    _width: Pixels,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let layout = app.workspaces.layout();
    let hints =
        app.preferences.always_show_shortcuts || window.modifiers().alt || app.open_menu.is_some();
    div()
        .id("tab-strip")
        .h(rems(style::BAR / 16.0))
        .flex_none()
        .bg(rgb(0x202020))
        .flex()
        .items_center()
        .overflow_x_scroll()
        .children(layout.tabs.iter().enumerate().map(|(index, tab)| {
            tab_button(app, index, (*tab).into(), *tab == layout.active, hints, cx)
        }))
        .child(
            div()
                .id("tab-strip-append")
                .flex_1()
                .min_w(rems(24.0 / 16.0))
                .h_full()
                .border_b_1()
                .border_color(rgb(style::BORDER))
                .child("")
                .drag_over::<DraggedTab>(|this, _, _, _| this.bg(rgb(0x383B3D)))
                .on_drop(cx.listener(move |app, drag: &DraggedTab, _, cx| {
                    reorder(app, drag, None, cx);
                })),
        )
}
