//! Compact, tabbed Docker run form. Parameter state and validation live in run.rs.
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use services::docker::{NetworkMode, RestartPolicy};

use super::run::RunSection;
use crate::app::Crabdash;
use crate::components::{
    common::{control_tooltip, lucide_icon, surface_button},
    style,
    text_field::TextField,
};
use crate::desktop::controls::{self, SurfaceControl};

fn form() -> Div {
    div().w_full().min_w_0().flex().flex_col().gap(px(14.0))
}

fn label(text: &'static str) -> Div {
    div()
        .text_size(rems(style::META / 16.0))
        .text_color(rgb(style::TEXT_MUTED))
        .child(text)
}

fn field(text: &'static str, input: &Entity<TextField>, hint: Option<&'static str>) -> Div {
    form()
        .gap(px(5.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(label(text))
                .when_some(hint, |row, hint| {
                    row.child(
                        div()
                            .id(SharedString::from(format!("run-hint-{text}")))
                            .text_color(rgb(style::TEXT_MUTED))
                            .child(lucide_icon(Icon::Info, 12.0))
                            .tooltip(move |_, cx| control_tooltip(hint, cx)),
                    )
                }),
        )
        .child(input.clone())
}

fn pair(left: Div, right: Div) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .gap(px(12.0))
        .child(left.flex_1())
        .child(right.flex_1())
}

fn choice(
    id: impl Into<ElementId>,
    text: &'static str,
    active: bool,
    cx: &mut Context<Crabdash>,
    change: impl Fn(&mut Crabdash) + 'static,
) -> SurfaceControl {
    let button = surface_button(id, None, Some(text))
        .bg(rgb(if active {
            style::CONTROL_SELECTED_BG
        } else {
            style::SURFACE
        }))
        .border_color(rgb(if active {
            style::CONTROL_SELECTED_BORDER
        } else {
            style::BORDER
        }))
        .text_color(rgb(if active {
            style::TEXT_SELECTED
        } else {
            style::TEXT_MUTED
        }))
        .on_click(cx.listener(move |this, _, _, cx| {
            if !this.docker_run_config.busy {
                change(this);
                this.docker_run_config.error = None;
                cx.notify();
            }
        }));
    controls::selected(button, active)
}

fn checkbox(
    id: &'static str,
    text: &'static str,
    hint: &'static str,
    active: bool,
    cx: &mut Context<Crabdash>,
    change: impl Fn(&mut Crabdash) + 'static,
) -> SurfaceControl {
    let button = div()
        .id(id)
        .h(rems(style::CONTROL / 16.0))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(7.0))
        .text_size(rems(style::META / 16.0))
        .text_color(rgb(style::TEXT_PRIMARY))
        .cursor_pointer()
        .child(lucide_icon(
            if active {
                Icon::SquareCheck
            } else {
                Icon::Square
            },
            style::ICON,
        ))
        .child(text)
        .tooltip(move |_, cx| control_tooltip(hint, cx))
        .on_click(cx.listener(move |this, _, _, cx| {
            if !this.docker_run_config.busy {
                change(this);
                this.docker_run_config.error = None;
                cx.notify();
            }
        }));
    controls::selected_button(
        button,
        text,
        Some(if active {
            Icon::SquareCheck
        } else {
            Icon::Square
        }),
        active,
    )
}

fn container(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let config = &app.docker_run_config;
    form()
        .child(field("Image (required)", &config.image, None))
        .child(field("Container name", &config.name, None))
        .child(field("Command", &config.command, Some("Optional. Overrides the image command. Quote arguments that contain spaces, for example: sh -c 'echo hello'.")))
        .child(form().gap(px(5.0)).child(label("Restart policy"))
            .child(div().flex().flex_wrap().gap(px(5.0)).children(RestartPolicy::all().iter().map(|&policy| {
                choice(SharedString::from(format!("run-restart-{}", policy.label())), policy.label(), config.restart == policy, cx, move |this| {
                    this.docker_run_config.restart = policy;
                    if policy != RestartPolicy::No { this.docker_run_config.remove = false; }
                })
            }))))
        .child(div().flex().flex_wrap().gap(px(20.0))
            .child(checkbox("run-toggle-remove", "Remove on exit", "Automatically remove the container when it exits (--rm). Cannot be combined with a restart policy.", config.remove, cx, |this| {
                this.docker_run_config.remove ^= true;
                if this.docker_run_config.remove { this.docker_run_config.restart = RestartPolicy::No; }
            }))
            .child(checkbox("run-toggle-interactive", "Keep stdin open", "Keep standard input open (-i). Containers run detached; use Terminal to attach.", config.interactive, cx, |this| this.docker_run_config.interactive ^= true)))
}

#[derive(Clone, Copy)]
enum Binding {
    Port,
    Volume,
    Environment,
}

impl Binding {
    fn kind(self) -> &'static str {
        match self {
            Self::Port => "port",
            Self::Volume => "volume",
            Self::Environment => "env",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Port => "Published ports",
            Self::Volume => "Volumes",
            Self::Environment => "Environment variables",
        }
    }
    fn add_label(self) -> &'static str {
        match self {
            Self::Port => "Add port",
            Self::Volume => "Add volume",
            Self::Environment => "Add variable",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Self::Port => "Host:container, for example 8080:80 or 127.0.0.1:8080:80/udp.",
            Self::Volume => {
                "Host path or volume name:container path. Add :ro for read-only access."
            }
            Self::Environment => {
                "One KEY=value per entry. Values can contain spaces; quotes are unnecessary."
            }
        }
    }
}

fn bindings(kind: Binding, app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let fields = match kind {
        Binding::Port => &app.docker_run_config.ports,
        Binding::Volume => &app.docker_run_config.volumes,
        Binding::Environment => &app.docker_run_config.env_vars,
    };
    form()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "run-binding-hint-{}",
                            kind.kind()
                        )))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(label(kind.label()))
                        .child(
                            div()
                                .text_color(rgb(style::TEXT_MUTED))
                                .child(lucide_icon(Icon::Info, 12.0)),
                        )
                        .tooltip(move |_, cx| control_tooltip(kind.hint(), cx)),
                )
                .child(
                    surface_button(
                        SharedString::from(format!("run-add-{}", kind.kind())),
                        Some(Icon::Plus),
                        Some(kind.add_label()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.docker_run_config.busy {
                            this.docker_run_config.add_field(kind.kind(), cx);
                            cx.notify();
                        }
                    })),
                ),
        )
        .when(fields.is_empty(), |body| {
            body.child(label(match kind {
                Binding::Port => "No ports published.",
                Binding::Volume => "No volumes mounted.",
                Binding::Environment => "Using the image's environment defaults.",
            }))
        })
        .children(fields.iter().enumerate().map(|(index, input)| {
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().flex_1().min_w_0().child(input.clone()))
                .child(
                    surface_button(
                        SharedString::from(format!("run-remove-{}-{index}", kind.kind())),
                        Some(Icon::X),
                        None,
                    )
                    .px(px(6.0))
                    .tooltip(|_, cx| control_tooltip("Remove entry", cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.docker_run_config.busy {
                            match kind {
                                Binding::Port => &mut this.docker_run_config.ports,
                                Binding::Volume => &mut this.docker_run_config.volumes,
                                Binding::Environment => &mut this.docker_run_config.env_vars,
                            }
                            .remove(index);
                            this.docker_run_config.error = None;
                            cx.notify();
                        }
                    })),
                )
        }))
}

fn connections(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    form()
        .gap(px(20.0))
        .child(
            form()
                .gap(px(5.0))
                .child(label("Network"))
                .child(
                    div()
                        .flex()
                        .gap(px(5.0))
                        .children(NetworkMode::all().iter().map(|&mode| {
                            choice(
                                SharedString::from(format!("run-network-{}", mode.label())),
                                mode.label(),
                                app.docker_run_config.network == mode,
                                cx,
                                move |this| this.docker_run_config.network = mode,
                            )
                        })),
                ),
        )
        .child(bindings(Binding::Port, app, cx))
        .child(bindings(Binding::Volume, app, cx))
}

fn advanced(app: &Crabdash) -> Div {
    let config = &app.docker_run_config;
    form()
        .child(pair(field("Memory limit", &config.memory, Some("Optional. Use a size such as 512m or 2g. Blank uses Docker's default.")), field("CPU limit", &config.cpus, Some("Optional. Use a positive number such as 0.5 or 2. Blank uses Docker's default."))))
        .child(pair(field("Hostname", &config.hostname, None), field("User", &config.user, None)))
        .child(pair(field("Working directory", &config.working_dir, None), field("Entrypoint", &config.entrypoint, None)))
        .child(label("Blank fields keep the image and Docker defaults."))
}

fn tabs(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    div()
        .flex_none()
        .px(px(18.0))
        .flex()
        .gap(px(18.0))
        .border_b_1()
        .border_color(rgb(style::BORDER))
        .children(RunSection::ALL.into_iter().map(|section| {
            let active = section == app.docker_run_config.section;
            let button = div()
                .id(SharedString::from(format!("run-tab-{}", section.label())))
                .h(rems(style::BAR / 16.0))
                .flex_none()
                .flex()
                .items_center()
                .border_b_2()
                .border_color(if active {
                    rgb(style::TAB_INDICATOR)
                } else {
                    rgba(0x00000000)
                })
                .text_size(rems(style::META / 16.0))
                .text_color(rgb(if active {
                    style::TEXT_SELECTED
                } else {
                    style::TEXT_MUTED
                }))
                .cursor_pointer()
                .hover(|s| s.text_color(rgb(style::TEXT_SELECTED)))
                .child(section.label())
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !this.docker_run_config.busy {
                        this.focus_handle.focus(window);
                        this.docker_run_config.section = section;
                        cx.notify();
                    }
                }));
            controls::selected_button(button, section.label(), None, active)
        }))
}

fn preview(app: &Crabdash, cx: &App) -> Stateful<Div> {
    let text = app
        .docker_run_config
        .build_args(cx)
        .map(|args| {
            format!(
                "docker run {}",
                shell_words::join(args.iter().map(String::as_str))
            )
        })
        .unwrap_or_else(|_| "Complete the required fields to preview the command.".into());
    div()
        .id("run-command-preview")
        .w_full()
        .min_w_0()
        .max_h(rems(72.0 / 16.0))
        .overflow_y_scroll()
        .text_size(rems(style::META / 16.0))
        .text_color(rgb(style::TEXT_MUTED))
        .font_family(app.preferences.terminal_font.clone())
        .whitespace_normal()
        .child(text)
}

pub fn render(app: &Crabdash, window: &Window, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let config = &app.docker_run_config;
    let busy = config.busy;
    let valid = config.build_args(cx).is_ok();
    let error = config.error.clone().or_else(|| {
        if config.image.read(cx).text().trim().is_empty() {
            None
        } else {
            config.build_args(cx).err()
        }
    });
    let content = match config.section {
        RunSection::Container => container(app, cx),
        RunSection::Connections => connections(app, cx),
        RunSection::Environment => bindings(Binding::Environment, app, cx),
        RunSection::Advanced => advanced(app),
    };
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
        .bg(rgba(0x00000088))
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(crate::desktop::controls::modal(
            div()
                .w(rems(540.0 / 16.0))
                .max_w(window.viewport_size().width - px(32.0))
                .h(rems(480.0 / 16.0))
                .max_h(window.viewport_size().height - px(40.0))
                .bg(rgb(style::SURFACE))
                .border_1()
                .border_color(rgb(style::BORDER))
                .rounded(px(style::CARD_RADIUS))
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_none()
                        .px(px(18.0))
                        .py(px(12.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(lucide_icon(Icon::Boxes, style::ICON))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(3.0))
                                        .child(
                                            div()
                                                .text_size(rems(style::TEXT / 16.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(style::TEXT_SELECTED))
                                                .child("Run container"),
                                        )
                                        .child(
                                            div()
                                                .text_size(rems(style::META / 16.0))
                                                .text_color(rgb(style::TEXT_MUTED))
                                                .child(
                                                    app.selected_machine()
                                                        .system_info
                                                        .machine_name
                                                        .clone(),
                                                ),
                                        ),
                                ),
                        )
                        .child(
                            surface_button("run-modal-close", Some(Icon::X), None)
                                .px(px(6.0))
                                .opacity(if busy { 0.5 } else { 1.0 })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_docker_run_modal(cx);
                                    if !this.docker_run_config.busy {
                                        this.focus_handle.focus(window);
                                    }
                                })),
                        ),
                )
                .child(tabs(app, cx))
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "run-panel-{}",
                            config.section.label()
                        )))
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .overflow_y_scroll()
                        .relative()
                        .child(div().w_full().p(px(18.0)).child(content))
                        .when(busy, |body| {
                            body.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
                                    .occlude()
                                    .bg(rgba(0x1E1E1E88))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    }),
                            )
                        }),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(18.0))
                        .py(px(12.0))
                        .border_t_1()
                        .border_color(rgb(style::BORDER))
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .when_some(error, |footer, error| {
                            footer.child(
                                div()
                                    .id("docker-run-error")
                                    .max_h(rems(48.0 / 16.0))
                                    .overflow_y_scroll()
                                    .text_size(rems(style::META / 16.0))
                                    .text_color(rgb(style::DANGER))
                                    .child(error),
                            )
                        })
                        .when(config.show_preview, |footer| footer.child(preview(app, cx)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.0))
                                .child(controls::selected_button(
                                    div()
                                        .id("run-preview-toggle")
                                        .h(rems(style::CONTROL / 16.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .text_size(rems(style::META / 16.0))
                                        .text_color(rgb(style::TEXT_MUTED))
                                        .cursor_pointer()
                                        .child(lucide_icon(
                                            if config.show_preview {
                                                Icon::ChevronDown
                                            } else {
                                                Icon::ChevronRight
                                            },
                                            12.0,
                                        ))
                                        .child("Command preview")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.docker_run_config.show_preview ^= true;
                                            cx.notify();
                                        })),
                                    "Command preview",
                                    Some(if config.show_preview {
                                        Icon::ChevronDown
                                    } else {
                                        Icon::ChevronRight
                                    }),
                                    config.show_preview,
                                ))
                                .child(controls::action_group(
                                    div().flex().gap(px(8.0)),
                                    vec![
                                        surface_button("run-modal-cancel", None, Some("Cancel"))
                                            .opacity(if busy { 0.5 } else { 1.0 })
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.close_docker_run_modal(cx);
                                                if !this.docker_run_config.busy {
                                                    this.focus_handle.focus(window);
                                                }
                                            })),
                                        controls::primary(surface_button(
                                            "run-modal-submit",
                                            Some(Icon::Play),
                                            Some(if busy { "Running…" } else { "Run" }),
                                        ))
                                        .opacity(if busy || !valid { 0.5 } else { 1.0 })
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.focus_handle.focus(window);
                                                this.submit_docker_run(cx);
                                            }),
                                        ),
                                    ],
                                )),
                        ),
                ),
            136.0 / 255.0,
        ))
}
