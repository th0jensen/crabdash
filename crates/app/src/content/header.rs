//! Joined tabs and their drag payload; layout state lives in workspaces.
use crate::app::{Crabdash, MainTab};
use crate::components::{
    common::{control_tooltip, lucide_icon},
    style,
};
use crate::features::workspaces::model::Drop;
use gpui::{prelude::*, *};

#[derive(Clone)]
pub(crate) struct DraggedTab {
    pub tab: MainTab,
    pub owner: EntityId,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(rems(32.0 / 16.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .rounded(px(style::CARD_RADIUS))
            .bg(rgb(style::SURFACE_HOVER))
            .border_1()
            .border_color(rgb(style::FOCUS_BORDER))
            .text_size(rems(style::TEXT / 16.0))
            .text_color(rgb(style::TEXT_SELECTED))
            .shadow_md()
            .child(tab_icon(self.tab))
            .child(self.tab.label())
    }
}

fn tab_icon(tab: MainTab) -> Div {
    if tab == MainTab::Docker {
        crate::features::docker::brand::icon(24.0)
    } else {
        lucide_icon(tab.icon(), style::ICON)
    }
}

fn tab_button(
    pane: u32,
    index: usize,
    tab: MainTab,
    active: bool,
    hints: bool,
    compact: bool,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let width = crate::features::preferences::current(cx).tab_width;
    let owner = cx.entity();
    div()
        .id(SharedString::from(format!(
            "pane-{pane}-tab-{}",
            tab.label()
        )))
        .h_full()
        .min_w(rems(
            if compact {
                if hints { 78.0 } else { 44.0 }
            } else {
                96.0
            } / 16.0,
        ))
        .max_w(rems(width / 16.0))
        .flex_1()
        .px(px(10.0))
        .relative()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(rems(style::TEXT / 16.0))
        .whitespace_nowrap()
        .text_color(rgb(if active {
            style::TEXT_SELECTED
        } else {
            style::TEXT_MUTED
        }))
        .border_t_1()
        .border_l_1()
        .border_r_1()
        .border_color(if active {
            rgb(style::BORDER)
        } else {
            rgba(0x00000000)
        })
        .rounded_t(px(style::CARD_RADIUS))
        .mb(-px(1.0))
        .bg(rgb(if active { 0x181818 } else { 0x161616 }))
        .when(active, |this| {
            this.child(
                div()
                    .absolute()
                    .bottom(-px(1.0))
                    .left_0()
                    .right_0()
                    .h(px(2.0))
                    .bg(rgb(0x181818)),
            )
        })
        .cursor_pointer()
        .hover(|this| {
            this.bg(rgb(style::SURFACE_HOVER))
                .text_color(rgb(style::TEXT_SELECTED))
        })
        .drag_over::<DraggedTab>(|this, _, _, _| {
            this.border_l_2().border_color(rgb(style::TAB_INDICATOR))
        })
        .tooltip(move |_, cx| {
            control_tooltip(
                format!("{} · drag to arrange; double-click to detach", tab.label()),
                cx,
            )
        })
        .child(tab_icon(tab))
        .when(!compact, |this| this.child(tab.label()))
        .child(div().flex_1())
        .when(hints, |this| {
            this.child(
                div()
                    .text_size(rems(style::META / 16.0))
                    .text_color(rgb(style::TEXT_MUTED))
                    .child(tab.shortcut()),
            )
        })
        .on_click(cx.listener(move |app, event: &ClickEvent, window, cx| {
            if event.click_count() == 2 {
                app.detach_workspace_tab(tab, window, cx);
            } else {
                app.select_pane_tab(pane, tab, cx);
            }
        }))
        .on_drag(
            DraggedTab {
                tab,
                owner: owner.entity_id(),
            },
            move |drag, _, _, cx| {
                owner.update(cx, |app, cx| {
                    app.workspaces.dragging_tab = true;
                    cx.notify();
                });
                cx.new(|_| drag.clone())
            },
        )
        .on_drop(cx.listener(move |app, drag: &DraggedTab, _, cx| {
            if drag.owner == cx.entity_id() {
                app.drop_workspace_tab(drag.tab, pane, Drop::Tab(index), cx);
            }
            cx.stop_propagation();
        }))
}

pub(super) fn render(
    app: &Crabdash,
    pane: u32,
    tabs: &[crate::features::workspaces::model::Tab],
    active: crate::features::workspaces::model::Tab,
    window: &Window,
    width: Pixels,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let hints =
        app.preferences.always_show_shortcuts || window.modifiers().alt || app.open_menu.is_some();
    let count = tabs.len();
    let compact =
        f32::from(width) / (f32::from(window.rem_size()) / 16.0) < tabs.len() as f32 * 104.0;
    div()
        .id(SharedString::from(format!("pane-{pane}-header")))
        .h(rems(40.0 / 16.0))
        .flex_none()
        .pt(px(5.0))
        .px(px(6.0))
        .bg(rgb(0x161616))
        .border_b_1()
        .border_color(rgb(style::BORDER))
        .flex()
        .items_center()
        .overflow_x_scroll()
        .gap(px(3.0))
        .children(tabs.iter().enumerate().map(|(index, tab)| {
            tab_button(
                pane,
                index,
                (*tab).into(),
                *tab == active,
                hints,
                compact,
                cx,
            )
        }))
        .child(div().flex_1().h_full())
        .on_drop(cx.listener(move |app, drag: &DraggedTab, _, cx| {
            if drag.owner == cx.entity_id() {
                app.drop_workspace_tab(drag.tab, pane, Drop::Tab(count), cx);
            }
        }))
}
