mod disks;
mod docker;
mod services;

use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::{Crabdash, MainTab};
use crate::components::common::{button, lucide_icon, machine_icon};

fn tab_button(tab: MainTab, active: bool, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let bg = if active { rgb(0x2C2C2E) } else { rgb(0x1C1C1E) };
    let color = if active { rgb(0xFFFFFF) } else { rgb(0xAEAEB2) };

    div()
        .id(SharedString::from(format!(
            "tab-{}",
            tab.label().to_lowercase()
        )))
        .flex_none()
        .h(px(34.0))
        .px(px(14.0))
        .bg(bg)
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .rounded(px(8.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x2A2A2C)))
        .child(
            div()
                .h_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_sm()
                .text_color(color)
                .child(lucide_icon(tab.icon(), 14.0))
                .child(tab.label().to_string()),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.active_tab = tab;
            this.refresh_services();
            cx.notify();
        }))
}

fn refresh_button(cx: &mut Context<Crabdash>) -> impl IntoElement {
    button("refresh-button", Icon::RefreshCw, "Refresh", false).on_click(cx.listener(
        |this, _, _, cx| {
            this.refresh_services();
            cx.notify();
        },
    ))
}

pub(super) fn status_badge(status: &str) -> Div {
    let is_running = status.contains("Running") || status.contains("Healthy");
    let status_bg = if is_running {
        rgb(0x193D2A)
    } else {
        rgb(0x47232B)
    };
    let status_fg = if is_running {
        rgb(0x30D158)
    } else {
        rgb(0xFF453A)
    };

    div()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(999.0))
        .bg(status_bg)
        .text_xs()
        .text_color(status_fg)
        .child(status.to_string())
}

pub(super) fn stats_chip(label: &str, value: String) -> Div {
    div()
        .h(px(34.0))
        .px(px(12.0))
        .py(px(7.0))
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(8.0))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x8E8E93))
                .child(label.to_string()),
        )
        .child(div().text_sm().text_color(white()).child(value))
}

pub(super) fn placeholder_card(title: &str, description: &str) -> Div {
    div()
        .bg(rgb(0x2C2C2E))
        .border_1()
        .border_color(rgb(0x3A3A3C))
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(div().text_sm().text_color(white()).child(title.to_string()))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xAEAEB2))
                .child(description.to_string()),
        )
}

pub(super) fn error_panel(title: &str, error: String) -> Div {
    div()
        .bg(rgb(0x47232B))
        .border_1()
        .border_color(rgb(0x5A2D35))
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xFF9F99))
                .child(title.to_string()),
        )
        .child(div().text_sm().text_color(rgb(0xF2B6B2)).child(error))
}

fn active_panel(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    match app.active_tab {
        MainTab::Docker => docker::render(app, cx),
        MainTab::Disks => disks::render(app, cx),
        MainTab::Services => services::render(app, cx),
    }
}

pub fn render_title_bar(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let selected_machine = app.selected_machine();

    div()
        .h(px(38.0))
        .px(px(14.0))
        .border_b_1()
        .border_color(rgb(0x2A2A2C))
        .bg(rgb(0x18181A))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .pl(px(72.0))
                .child(
                    div()
                        .id("toggle-sidebar")
                        .w(px(20.0))
                        .h(px(20.0))
                        .rounded(px(6.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(0xE5E5EA))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x2C2C2E)).text_color(rgb(0xE5E5EA)))
                        .child(lucide_icon(
                            if app.sidebar_collapsed {
                                Icon::PanelLeftOpen
                            } else {
                                Icon::PanelLeftClose
                            },
                            12.0,
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_sidebar(cx);
                        })),
                )
                .text_sm()
                .text_color(rgb(0xE5E5EA))
                .child("Crabdash"),
        )
        .child(
            div()
                .px(px(10.0))
                .py(px(5.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_sm()
                .text_color(rgb(0xE5E5EA))
                .child(lucide_icon(machine_icon(selected_machine.kind), 12.0))
                .child(format!("{}", selected_machine.system_info.machine_name,)),
        )
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .bg(rgb(0x1E1E20))
        .child(
            div()
                .px(px(20.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(rgb(0x3A3A3C))
                .flex()
                .gap(px(10.0))
                .child(tab_button(
                    MainTab::Docker,
                    app.active_tab == MainTab::Docker,
                    cx,
                ))
                .child(tab_button(
                    MainTab::Disks,
                    app.active_tab == MainTab::Disks,
                    cx,
                ))
                .child(tab_button(
                    MainTab::Services,
                    app.active_tab == MainTab::Services,
                    cx,
                )),
        )
        .child(
            div()
                .flex_1()
                .bg(rgb(0x1E1E20))
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(20.0))
                        .py(px(14.0))
                        .border_b_1()
                        .border_color(rgb(0x3A3A3C))
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .mt(px(2.0))
                                        .text_color(rgb(0x8AB4FF))
                                        .child(lucide_icon(app.active_tab.icon(), 18.0)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(
                                            div()
                                                .text_xl()
                                                .text_color(white())
                                                .child(app.active_tab.label()),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(rgb(0x8E8E93))
                                                .child(app.active_tab.subtitle()),
                                        ),
                                ),
                        )
                        .child(if app.active_tab == MainTab::Docker {
                            refresh_button(cx).into_any_element()
                        } else {
                            div().into_any_element()
                        }),
                )
                .child(div().flex_1().p(px(20.0)).child(active_panel(app, cx))),
        )
}
