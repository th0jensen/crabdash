//! A fixed add control belongs to each dashboard group, outside its tab scroll.
use super::tab_icon;
use crate::{
    app::{Crabdash, MainTab},
    components::{
        common::{control_tooltip, lucide_icon},
        style,
    },
    features::workspaces::groups::{Picker, Target},
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use std::{cell::Cell, rc::Rc};

pub(super) fn button(
    app: &Crabdash,
    pane: u32,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let target = Target::current(app, pane);
    let control = window.rem_size() * (style::BAR / 16.0);
    let picker = app.workspaces.group_picker.filter(|picker| {
        picker.target == target
            && picker.accepts(
                app.workspaces.store.active,
                app.workspaces.revision(),
                app.workspaces.layout(),
            )
    });
    let available = !app.workspaces.layout().available_features(pane).is_empty();
    let button_bounds = Rc::new(Cell::new(None));
    let measured_bounds = button_bounds.clone();
    div()
        .id(SharedString::from(format!("pane-{pane}-add-dashboard")))
        .h_full()
        .w(control)
        .flex_none()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .border_b_1()
        .border_color(crate::components::style::rgb(style::BORDER))
        .text_color(crate::components::style::rgb(style::TEXT_MUTED))
        .when(available, |this| {
            this.cursor_pointer().hover(|this| {
                this.bg(crate::components::style::rgb(style::SURFACE_HOVER))
                    .text_color(crate::components::style::rgb(style::TEXT_PRIMARY))
            })
        })
        .when(!available, |this| this.opacity(0.4))
        .when(picker.is_none(), |this| {
            this.tooltip(move |_, cx| {
                control_tooltip(
                    if available {
                        "Add dashboard tab"
                    } else {
                        "All dashboards are open in this split"
                    },
                    cx,
                )
            })
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |app, _, window, cx| {
            if available {
                app.toggle_dashboard_picker(target, window, cx);
            }
            cx.stop_propagation();
        }))
        .child(lucide_icon(Icon::Plus, style::ICON))
        .child(
            canvas(
                move |bounds, _, _| measured_bounds.set(Some(bounds)),
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .when_some(picker, |this, picker| {
            this.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .size(px(0.0))
                    .child(deferred(
                        anchored()
                            .position_mode(AnchoredPositionMode::Local)
                            .anchor(Corner::TopLeft)
                            .position(point(px(0.0), control))
                            .snap_to_window_with_margin(px(8.0))
                            .child(menu(app, picker, button_bounds, cx)),
                    )),
            )
        })
}

fn menu(
    app: &Crabdash,
    picker: Picker,
    button_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!(
            "pane-{}-dashboard-picker",
            picker.target.pane
        )))
        .w(rems(180.0 / 16.0))
        .p(rems(4.0 / 16.0))
        .rounded(px(style::RADIUS))
        .bg(crate::components::style::rgb(style::SURFACE))
        .border_1()
        .border_color(crate::components::style::rgb(style::BORDER))
        .shadow_lg()
        .occlude()
        .flex()
        .flex_col()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down_out(cx.listener(move |app, event: &MouseDownEvent, _, cx| {
            // Let the owning Plus toggle the open picker itself. Otherwise its
            // mouse-down clears the menu before its click can close it.
            if button_bounds
                .get()
                .is_some_and(|bounds| bounds.contains(&event.position))
            {
                return;
            }
            app.workspaces.group_picker = None;
            cx.notify();
        }))
        .on_key_down(cx.listener(|app, event: &KeyDownEvent, _, cx| {
            if event.keystroke.key == "escape" {
                app.workspaces.group_picker = None;
                cx.notify();
                cx.stop_propagation();
            }
        }))
        .children(
            app.workspaces
                .layout()
                .available_features(picker.target.pane)
                .into_iter()
                .map(|tab| {
                    let kind: MainTab = tab.into();
                    div()
                        .id(SharedString::from(format!(
                            "add-dashboard-{}-{}",
                            picker.target.pane,
                            kind.label()
                        )))
                        .h(rems(style::CONTROL / 16.0))
                        .px(rems(8.0 / 16.0))
                        .rounded(px(style::RADIUS))
                        .flex()
                        .items_center()
                        .gap(rems(8.0 / 16.0))
                        .text_size(rems(style::TEXT / 16.0))
                        .text_color(crate::components::style::rgb(style::TEXT_PRIMARY))
                        .cursor_pointer()
                        .hover(|this| this.bg(crate::components::style::rgb(style::SURFACE_HOVER)))
                        .child(tab_icon(kind, style::color(style::TEXT_PRIMARY)))
                        .child(kind.label())
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.add_dashboard_feature(picker, tab, cx);
                            cx.stop_propagation();
                        }))
                }),
        )
}
