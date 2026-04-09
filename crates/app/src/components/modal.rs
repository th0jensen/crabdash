use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;

use crate::app::{AddMachineAuthMode, Crabdash};
use crate::components::common::{button, lucide_icon};


fn auth_mode_button(
    app: &Crabdash,
    mode: AddMachineAuthMode,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let selected = app.add_machine_auth_mode == mode;

    div()
        .id(SharedString::from(format!("auth-mode-{}", mode.label())))
        .h(px(34.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(if selected {
            rgb(0x0A84FF)
        } else {
            rgb(0x2F2F31)
        })
        .bg(if selected {
            rgb(0x1F3656)
        } else {
            rgb(0x2C2C2E)
        })
        .text_sm()
        .text_color(if selected {
            rgb(0xFFFFFF)
        } else {
            rgb(0xAEAEB2)
        })
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x3A3A3C)))
        .child(mode.label())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.set_add_machine_auth_mode(mode, cx);
        }))
}

fn auth_field(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xAEAEB2))
                .child("Authentication Method"),
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(auth_mode_button(app, AddMachineAuthMode::None, cx))
                .child(auth_mode_button(app, AddMachineAuthMode::Password, cx))
                .child(auth_mode_button(app, AddMachineAuthMode::AuthKey, cx)),
        )
}

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
                                .border_color(rgb(0x2F2F31))
                                .rounded(px(14.0))
                                .overflow_hidden()
                                .on_action(cx.listener(Crabdash::focus_next))
                                .on_action(cx.listener(Crabdash::focus_prev))
                                .child(
                                    div()
                                        .px(px(18.0))
                                        .py(px(16.0))
                                        .border_b_1()
                                        .border_color(rgb(0x2F2F31))
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
                                )
                                .child(
                                    div()
                                        .p(px(18.0))
                                        .flex()
                                        .flex_col()
                                        .gap(px(12.0))
                                        .child(app.remote_host_field.clone())
                                        .child(app.remote_user_field.clone())
                                        .child(auth_field(app, cx))
                                        .when(
                                            app.add_machine_auth_mode
                                                == AddMachineAuthMode::Password,
                                            |this| this.child(app.remote_password_field.clone()),
                                        )
                                        .when(
                                            app.add_machine_auth_mode
                                                == AddMachineAuthMode::AuthKey,
                                            |this| {
                                                this.child(app.remote_private_key_field.clone())
                                                    .child(app.remote_public_key_field.clone())
                                                    .child(app.remote_passphrase_field.clone())
                                            },
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0xAEAEB2))
                                                .when(app.add_machine_auth_mode
                                                   != AddMachineAuthMode::None,
                                                |this| this.child("Passwords and SSH key passphrases are stored in your local encrypted keychain and not uploaded anywhere."))
                                                .when(app.add_machine_auth_mode
                                                   == AddMachineAuthMode::None,
                                                |this| this.child("This method allows for connecting without a password using for example Tailscale or WireGuard."))
                                        )
                                        .when_some(app.add_machine_error.as_ref(), |this, error| {
                                            this.child(
                                                div()
                                                    .p(px(10.0))
                                                    .rounded(px(8.0))
                                                    .bg(rgb(0x47232B))
                                                    .border_1()
                                                    .border_color(rgb(0x4A252C))
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
                                        .border_color(rgb(0x2F2F31))
                                        .flex()
                                        .justify_end()
                                        .gap(px(10.0))
                                        .child(
                                            button("cancel-add-machine", Icon::X, Some("Cancel"), false).on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    this.close_add_machine_modal(window, cx);
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
