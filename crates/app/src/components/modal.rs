use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::Crabdash;
use crate::components::common::{button, lucide_icon};

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(rgba(0x00000099)),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .flex_col()
                .justify_around()
                .child(
                    div()
                        .w_full()
                        .flex()
                        .justify_around()
                        .child(
                            div()
                                .w(px(440.0))
                                .bg(rgb(0x1C1C1E))
                                .border_1()
                                .border_color(rgb(0x3A3A3C))
                                .rounded(px(14.0))
                                .overflow_hidden()
                                .on_action(cx.listener(Crabdash::focus_next))
                                .on_action(cx.listener(Crabdash::focus_prev))
                                .child(
                                    div()
                                        .px(px(18.0))
                                        .py(px(16.0))
                                        .border_b_1()
                                        .border_color(rgb(0x3A3A3C))
                                        .flex()
                                        .justify_between()
                                        .items_start()
                                        .gap(px(12.0))
                                        .child(
                                            div()
                                                .mt(px(2.0))
                                                .text_color(rgb(0x8AB4FF))
                                                .child(lucide_icon(Icon::Server, 16.0)),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .flex()
                                                .flex_wrap()
                                                .gap(px(4.0))
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(white())
                                                        .child("Add New Machine"),
                                                )
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(0x8E8E93))
                                                        .child(
                                                            "Connect over SSH and capture its system information.",
                                                        ),
                                                ),
                                        )
                                        // .child(
                                        //     button("close-add-machine-modal", Icon::X, "Close", false)
                                        //         .on_click(cx.listener(
                                        //             |this, _, _, cx| this.close_add_machine_modal(cx),
                                        //         )),
                                        // ),
                                )
                                .child(
                                    div()
                                        .p(px(18.0))
                                        .flex()
                                        .flex_col()
                                        .gap(px(12.0))
                                        .child(app.remote_host_field.clone())
                                        .child(app.remote_user_field.clone())
                                        .child(app.remote_password_field.clone())
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0xAEAEB2))
                                                .child("Your passwords will be stored in your local encrypted keychain and not uploaded anywhere."),
                                        )
                                        .when_some(app.add_machine_error.as_ref(), |this, error| {
                                            this.child(
                                                div()
                                                    .p(px(10.0))
                                                    .rounded(px(8.0))
                                                    .bg(rgb(0x47232B))
                                                    .border_1()
                                                    .border_color(rgb(0x5A2D35))
                                                    .text_xs()
                                                    .text_color(rgb(0xFF9F99))
                                                    .child(error.to_string()),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .p(px(18.0))
                                        .border_t_1()
                                        .border_color(rgb(0x3A3A3C))
                                        .flex()
                                        .justify_end()
                                        .gap(px(10.0))
                                        .child(
                                            button("cancel-add-machine", Icon::X, Some("Cancel"), false).on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.close_add_machine_modal(cx);
                                                }),
                                            ),
                                        )
                                        .child(
                                            button("submit-add-machine", Icon::Plus, Some("Add Machine"), true)
                                                .on_click(cx.listener(
                                                    |this, _, window, cx| {
                                                        this.submit_add_machine(window, cx);
                                                    },
                                                )),
                                        ),
                                ),
                        ),
                ),
        )
}
