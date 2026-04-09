use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use services::docker::{NetworkMode, RestartPolicy};

use crate::app::Crabdash;
use crate::components::common::{LucideIcon, button, lucide_icon};
use crate::components::text_field::TextField;

// ── helpers ──────────────────────────────────────────────────────────────────

fn section_label(text: &'static str) -> Div {
    div()
        .text_xs()
        .text_color(rgb(0x8E8E93))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}

fn field_label(text: &'static str) -> Div {
    div().text_xs().text_color(rgb(0x8E8E93)).child(text)
}

fn toggle_btn(
    id: impl Into<ElementId>,
    label: &'static str,
    active: bool,
    cx: &mut Context<Crabdash>,
    on_click: impl Fn(&mut Crabdash, &mut Context<Crabdash>) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(if active { rgb(0x0A84FF) } else { rgb(0x2F2F31) })
        .bg(if active { rgb(0x1F3656) } else { rgb(0x2C2C2E) })
        .text_xs()
        .text_color(if active { rgb(0xFFFFFF) } else { rgb(0xAEAEB2) })
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x3A3A3C)))
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            on_click(this, cx);
        }))
}

fn labeled_field(label: &'static str, field: Entity<TextField>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(field_label(label))
        .child(field)
}

fn section_divider() -> Div {
    div().w_full().h(px(1.0)).bg(rgb(0x2F2F31))
}

fn list_remove_btn(
    id: impl Into<ElementId>,
    cx: &mut Context<Crabdash>,
    on_click: impl Fn(&mut Crabdash, &mut Context<Crabdash>) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.0))
        .w(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(0x2F2F31))
        .bg(rgb(0x2C2C2E))
        .text_color(rgb(0x8E8E93))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x3A3A3C)).text_color(rgb(0xFF453A)))
        .child(lucide_icon(Icon::X, 11.0))
        .on_click(cx.listener(move |this, _, _, cx| {
            on_click(this, cx);
        }))
}

fn add_entry_btn(
    id: impl Into<ElementId>,
    label: &'static str,
    cx: &mut Context<Crabdash>,
    on_click: impl Fn(&mut Crabdash, &mut Context<Crabdash>) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(0x2F2F31))
        .bg(rgb(0x242426))
        .text_xs()
        .text_color(rgb(0x8E8E93))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x2C2C2E)).text_color(rgb(0xFFFFFF)))
        .child(lucide_icon(Icon::Plus, 11.0))
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            on_click(this, cx);
        }))
}

// ── sections ─────────────────────────────────────────────────────────────────

fn section_image(app: &Crabdash) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(field_label("Image"))
                .child(div().text_xs().text_color(rgb(0xFF453A)).child("*")),
        )
        .child(app.docker_run_config.image.clone())
}

fn section_identity(app: &Crabdash) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(section_label("Identity"))
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .child(labeled_field("Name", app.docker_run_config.name.clone()).flex_1())
                .child(labeled_field("Hostname", app.docker_run_config.hostname.clone()).flex_1()),
        )
}

fn section_behavior(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(section_label("Behavior"))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .child(toggle_btn(
                    "run-toggle-detach",
                    "Detach (-d)",
                    app.docker_run_config.detach,
                    cx,
                    |this, cx| {
                        this.docker_run_config.detach = !this.docker_run_config.detach;
                        cx.notify();
                    },
                ))
                .child(toggle_btn(
                    "run-toggle-interactive",
                    "Interactive (-it)",
                    app.docker_run_config.interactive,
                    cx,
                    |this, cx| {
                        this.docker_run_config.interactive = !this.docker_run_config.interactive;
                        cx.notify();
                    },
                ))
                .child(toggle_btn(
                    "run-toggle-remove",
                    "Remove on exit (--rm)",
                    app.docker_run_config.remove,
                    cx,
                    |this, cx| {
                        this.docker_run_config.remove = !this.docker_run_config.remove;
                        cx.notify();
                    },
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(field_label("Restart Policy"))
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .children(RestartPolicy::all().iter().map(|&policy| {
                            let active = app.docker_run_config.restart == policy;
                            toggle_btn(
                                SharedString::from(format!("run-restart-{}", policy.label())),
                                policy.label(),
                                active,
                                cx,
                                move |this, cx| {
                                    this.docker_run_config.restart = policy;
                                    cx.notify();
                                },
                            )
                        })),
                ),
        )
}

fn section_ports(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(section_label("Ports"))
        .children(
            app.docker_run_config
                .ports
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(field.clone())
                        .child(list_remove_btn(
                            SharedString::from(format!("run-remove-port-{i}")),
                            cx,
                            move |this, cx| {
                                this.docker_run_config.ports.remove(i);
                                cx.notify();
                            },
                        ))
                }),
        )
        .child(add_entry_btn(
            "run-add-port",
            "Add port (host:container[/tcp|udp])",
            cx,
            |this, cx| {
                let field = cx.new(|cx| TextField::new("", "8080:80", 0, cx));
                this.docker_run_config.ports.push(field);
                cx.notify();
            },
        ))
}

fn section_volumes(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(section_label("Volumes"))
        .children(
            app.docker_run_config
                .volumes
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(field.clone())
                        .child(list_remove_btn(
                            SharedString::from(format!("run-remove-volume-{i}")),
                            cx,
                            move |this, cx| {
                                this.docker_run_config.volumes.remove(i);
                                cx.notify();
                            },
                        ))
                }),
        )
        .child(add_entry_btn(
            "run-add-volume",
            "Add volume (/host/path:/container/path[:ro])",
            cx,
            |this, cx| {
                let field = cx.new(|cx| TextField::new("", "/host/path:/container/path", 0, cx));
                this.docker_run_config.volumes.push(field);
                cx.notify();
            },
        ))
}

fn section_env(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(section_label("Environment"))
        .children(
            app.docker_run_config
                .env_vars
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(field.clone())
                        .child(list_remove_btn(
                            SharedString::from(format!("run-remove-env-{i}")),
                            cx,
                            move |this, cx| {
                                this.docker_run_config.env_vars.remove(i);
                                cx.notify();
                            },
                        ))
                }),
        )
        .child(add_entry_btn(
            "run-add-env",
            "Add variable (KEY=VALUE)",
            cx,
            |this, cx| {
                let field = cx.new(|cx| TextField::new("", "KEY=VALUE", 0, cx));
                this.docker_run_config.env_vars.push(field);
                cx.notify();
            },
        ))
}

fn section_resources(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(section_label("Resources & Network"))
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .child(labeled_field("Memory", app.docker_run_config.memory.clone()).flex_1())
                .child(labeled_field("CPUs", app.docker_run_config.cpus.clone()).flex_1())
                .child(labeled_field("User", app.docker_run_config.user.clone()).flex_1()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(field_label("Network Mode"))
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .children(NetworkMode::all().iter().map(|&mode| {
                            let active = app.docker_run_config.network == mode;
                            toggle_btn(
                                SharedString::from(format!("run-network-{}", mode.label())),
                                mode.label(),
                                active,
                                cx,
                                move |this, cx| {
                                    this.docker_run_config.network = mode;
                                    cx.notify();
                                },
                            )
                        })),
                ),
        )
}

fn section_advanced(app: &Crabdash) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(section_label("Advanced"))
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .child(
                    labeled_field("Working Dir", app.docker_run_config.working_dir.clone())
                        .flex_1(),
                )
                .child(
                    labeled_field("Entrypoint", app.docker_run_config.entrypoint.clone()).flex_1(),
                ),
        )
        .child(labeled_field(
            "Command Override",
            app.docker_run_config.command.clone(),
        ))
}

fn command_preview(app: &Crabdash, cx: &App) -> Div {
    let args = app.docker_run_config.build_args(cx);
    let preview = format!("docker run {}", args.join(" "));

    div()
        .w_full()
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(rgb(0x151517))
        .border_1()
        .border_color(rgb(0x2F2F31))
        .text_xs()
        .text_color(rgb(0x8E8E93))
        .font_family("JetBrainsMono Nerd Font")
        .child(preview)
}

// ── public entry point ────────────────────────────────────────────────────────

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        // Backdrop
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(rgba(0x00000099)),
        )
        // Modal
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(560.0))
                        .max_h(px(780.0))
                        .bg(rgb(0x1C1C1E))
                        .border_1()
                        .border_color(rgb(0x2F2F31))
                        .rounded(px(14.0))
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        // Header
                        .child(
                            div()
                                .px(px(18.0))
                                .py(px(14.0))
                                .border_b_1()
                                .border_color(rgb(0x2F2F31))
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(10.0))
                                        .child(
                                            div()
                                                .text_color(rgb(0x8AB4FF))
                                                .child(lucide_icon(Icon::Play, 14.0)),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(white())
                                                .child("Run Container"),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("run-modal-close")
                                        .h(px(28.0))
                                        .w(px(28.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(6.0))
                                        .bg(rgb(0x2C2C2E))
                                        .text_color(rgb(0xAEAEB2))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(0x3A3A3C)))
                                        .child(lucide_icon(Icon::X, 14.0))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_docker_run_modal(cx);
                                        })),
                                ),
                        )
                        // Scrollable body
                        .child(
                            div()
                                .id("run-modal-body")
                                .flex_1()
                                .overflow_y_scroll()
                                .px(px(18.0))
                                .py(px(16.0))
                                .flex()
                                .flex_col()
                                .gap(px(16.0))
                                .child(section_image(app))
                                .child(section_divider())
                                .child(section_identity(app))
                                .child(section_divider())
                                .child(section_behavior(app, cx))
                                .child(section_divider())
                                .child(section_ports(app, cx))
                                .child(section_divider())
                                .child(section_volumes(app, cx))
                                .child(section_divider())
                                .child(section_env(app, cx))
                                .child(section_divider())
                                .child(section_resources(app, cx))
                                .child(section_divider())
                                .child(section_advanced(app)),
                        )
                        // Footer: preview + actions
                        .child(
                            div()
                                .px(px(18.0))
                                .py(px(14.0))
                                .border_t_1()
                                .border_color(rgb(0x2F2F31))
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .child(command_preview(app, cx))
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .gap(px(10.0))
                                        .child(
                                            button(
                                                "run-modal-cancel",
                                                Option::<LucideIcon>::None,
                                                Some("Cancel"),
                                                false,
                                            )
                                            .on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.close_docker_run_modal(cx);
                                                }),
                                            ),
                                        )
                                        .child(
                                            button(
                                                "run-modal-submit",
                                                Some(Icon::Play),
                                                Some("Run Container"),
                                                true,
                                            )
                                            .on_click(
                                                cx.listener(|this, _, _, cx| {
                                                    this.submit_docker_run(cx);
                                                }),
                                            ),
                                        ),
                                ),
                        ),
                ),
        )
}
