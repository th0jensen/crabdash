mod commands;
mod models;

use commands::{SystemInfo, list_local_docker};
use gpui::prelude::*;
use gpui::*;
use models::{Machine, MachineKind, ServiceItem};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MainTab {
    Docker,
    Disks,
    Services,
}

impl MainTab {
    fn label(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Disks => "Disks",
            Self::Services => "Services",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::Docker => "Containers and workloads",
            Self::Disks => "Volumes and storage health",
            Self::Services => "Units and background processes",
        }
    }
}

struct Crabdash {
    machines: Vec<Machine>,
    selected_machine: usize,
    active_tab: MainTab,
    services: Vec<ServiceItem>,
    error: Option<String>,
}

impl Crabdash {
    fn new() -> Self {
        let local_system_info = SystemInfo::local().unwrap_or_else(|_| SystemInfo {
            machine_name: "This Mac".into(),
            os_version: MachineKind::MacOS.label().into(),
            arch: String::new(),
        });

        let mut app = Self {
            machines: vec![
                Machine {
                    id: "local".into(),
                    system_info: local_system_info,
                    kind: MachineKind::MacOS,
                },
                Machine {
                    id: "build-01".into(),
                    system_info: SystemInfo {
                        machine_name: "build-01".into(),
                        os_version: MachineKind::Linux.label().into(),
                        arch: String::new(),
                    },
                    kind: MachineKind::Linux,
                },
                Machine {
                    id: "edge-eu".into(),
                    system_info: SystemInfo {
                        machine_name: "edge-eu".into(),
                        os_version: MachineKind::Linux.label().into(),
                        arch: String::new(),
                    },
                    kind: MachineKind::Linux,
                },
            ],
            selected_machine: 0,
            active_tab: MainTab::Docker,
            services: Vec::new(),
            error: None,
        };
        app.refresh_services();
        app
    }

    fn selected_machine(&self) -> &Machine {
        &self.machines[self.selected_machine]
    }

    fn selected_machine_is_local(&self) -> bool {
        self.selected_machine().id == "local"
    }

    fn refresh_services(&mut self) {
        if !self.selected_machine_is_local() || self.active_tab != MainTab::Docker {
            self.services.clear();
            self.error = None;
            return;
        }

        match list_local_docker() {
            Ok(services) => {
                self.services = services;
                self.error = None;
            }
            Err(error) => {
                self.services.clear();
                self.error = Some(error);
            }
        }
    }

    fn machine_item(
        machine: &Machine,
        index: usize,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let name_color = if selected {
            rgb(0xFFFFFF)
        } else {
            rgb(0xD1D1D6)
        };
        let meta_color = if selected {
            rgb(0x0A84FF)
        } else {
            rgb(0x8E8E93)
        };
        let bg = if selected {
            rgb(0x2C2C2E)
        } else {
            rgb(0x1C1C1E)
        };
        let border = rgb(0x3A3A3C);
        let dot = match machine.kind {
            MachineKind::MacOS => rgb(0x64D2FF),
            MachineKind::Linux => rgb(0x30D158),
        };

        div()
            .id(SharedString::from(format!("machine-{}", machine.id)))
            .w_full()
            .p(px(10.0))
            .bg(bg)
            .border_b_1()
            .border_color(border)
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x2A2A2C)))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(name_color)
                                    .child(machine.system_info.machine_name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(meta_color)
                                    .child(machine.system_info.os_version.clone()),
                            ),
                    )
                    .child(div().w(px(8.0)).h(px(8.0)).rounded(px(999.0)).bg(dot)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_machine = index;
                this.refresh_services();
                cx.notify();
            }))
    }

    fn tab_button(tab: MainTab, active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = if active { rgb(0x2C2C2E) } else { rgb(0x1C1C1E) };
        let color = if active { rgb(0xFFFFFF) } else { rgb(0xAEAEB2) };

        div()
            .id(SharedString::from(format!(
                "tab-{}",
                tab.label().to_lowercase()
            )))
            .flex_1()
            .h(px(34.0))
            .bg(bg)
            .border_r_1()
            .border_color(rgb(0x3A3A3C))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x2A2A2C)))
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(color)
                    .child(tab.label().to_string()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active_tab = tab;
                this.refresh_services();
                cx.notify();
            }))
    }

    fn service_row(service: &ServiceItem) -> Div {
        let is_running = service.status.contains("running") || service.status.contains("healthy");
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
            .w_full()
            .px(px(14.0))
            .py(px(12.0))
            .border_b_1()
            .border_color(rgb(0x3A3A3C))
            .flex()
            .justify_between()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_sm()
                            .text_color(white())
                            .child(service.name.clone()),
                    )
                    .child(div().text_xs().text_color(rgb(0x8E8E93)).child(format!(
                        "{} • {}",
                        service.kind.label(),
                        service.id
                    ))),
            )
            .child(
                div()
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(999.0))
                    .bg(status_bg)
                    .text_xs()
                    .text_color(status_fg)
                    .child(service.status.clone()),
            )
    }

    fn refresh_button(cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("refresh-button")
            .px(px(12.0))
            .py(px(8.0))
            .bg(rgb(0x2C2C2E))
            .border_1()
            .border_color(rgb(0x3A3A3C))
            .text_sm()
            .text_color(rgb(0xE5E5EA))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x3A3A3C)))
            .child("Refresh")
            .on_click(cx.listener(|this, _, _, cx| {
                this.refresh_services();
                cx.notify();
            }))
    }

    fn stats_chip(label: &str, value: String) -> Div {
        div()
            .px(px(10.0))
            .py(px(8.0))
            .bg(rgb(0x2C2C2E))
            .border_1()
            .border_color(rgb(0x3A3A3C))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x8E8E93))
                    .child(label.to_string()),
            )
            .child(div().text_sm().text_color(white()).child(value))
    }

    fn placeholder_card(title: &str, description: &str) -> Div {
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

    fn docker_panel(&self) -> Div {
        let running_count = self
            .services
            .iter()
            .filter(|service| {
                service.status.contains("running") || service.status.contains("healthy")
            })
            .count();

        if !self.selected_machine_is_local() {
            return Self::placeholder_card(
                "Remote Docker view not wired yet",
                "The machine sidebar is ready, but remote Docker transport still needs to be implemented.",
            );
        }

        if let Some(error) = &self.error {
            return div()
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
                        .child("Unable to load Docker"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xF2B6B2))
                        .child(error.clone()),
                );
        }

        if self.services.is_empty() {
            return Self::placeholder_card(
                "No Docker containers found",
                "The local Docker command ran successfully but did not return any containers.",
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(Self::stats_chip("Total", self.services.len().to_string()))
                    .child(Self::stats_chip("Running", running_count.to_string())),
            )
            .child(
                div()
                    .id("docker-scroll")
                    .bg(rgb(0x2C2C2E))
                    .border_1()
                    .border_color(rgb(0x3A3A3C))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .px(px(14.0))
                            .py(px(12.0))
                            .border_b_1()
                            .border_color(rgb(0x3A3A3C))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x8E8E93))
                                    .child("DOCKER CONTAINERS"),
                            ),
                    )
                    .children(self.services.iter().map(Self::service_row)),
            )
    }

    fn active_panel(&self) -> Div {
        match self.active_tab {
            MainTab::Docker => self.docker_panel(),
            MainTab::Disks => Self::placeholder_card(
                "Disk view coming next",
                "The top tabs are in place, but the disk backend still needs to be implemented.",
            ),
            MainTab::Services => Self::placeholder_card(
                "Services view coming next",
                "This tab is intended for system services once the backend commands exist.",
            ),
        }
    }

    fn title_bar(&self) -> Div {
        let selected_machine = self.selected_machine();

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
                    .pl(px(72.0))
                    .text_sm()
                    .text_color(rgb(0x8E8E93))
                    .child("crabdash"),
            )
            .child(
                div()
                    .px(px(10.0))
                    .py(px(5.0))
                    .text_sm()
                    .text_color(rgb(0xE5E5EA))
                    .child(format!(
                        "{} | {}",
                        selected_machine.system_info.machine_name,
                        selected_machine.system_info.os_version
                    )),
            )
    }
}

impl Render for Crabdash {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title("Crabdash");

        let machine_entries: Vec<_> = self
            .machines
            .iter()
            .enumerate()
            .map(|(index, machine)| {
                Self::machine_item(machine, index, self.selected_machine == index, cx)
                    .into_any_element()
            })
            .collect();

        div()
            .size_full()
            .bg(rgb(0x18181A))
            .text_color(white())
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        self.title_bar()
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move();
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .child(
                                div()
                                    .w(px(250.0))
                                    .h_full()
                                    .bg(rgb(0x1C1C1E))
                                    .border_r_1()
                                    .border_color(rgb(0x3A3A3C))
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .h(px(34.0))
                                            .px(px(10.0))
                                            .border_b_1()
                                            .border_color(rgb(0x3A3A3C))
                                            .flex()
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x8E8E93))
                                                    .child("MACHINES"),
                                            ),
                                    )
                                    .children(machine_entries)
                                    .child(div().flex_1())
                                    .into_any_element(),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .bg(rgb(0x1E1E20))
                                    .child(
                                        div()
                                            .h(px(34.0))
                                            .border_b_1()
                                            .border_color(rgb(0x3A3A3C))
                                            .flex()
                                            .child(Self::tab_button(
                                                MainTab::Docker,
                                                self.active_tab == MainTab::Docker,
                                                cx,
                                            ))
                                            .child(Self::tab_button(
                                                MainTab::Disks,
                                                self.active_tab == MainTab::Disks,
                                                cx,
                                            ))
                                            .child(Self::tab_button(
                                                MainTab::Services,
                                                self.active_tab == MainTab::Services,
                                                cx,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .id("content-scroll")
                                            .flex_1()
                                            .overflow_y_scroll()
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
                                                            .flex_col()
                                                            .gap(px(4.0))
                                                            .child(
                                                                div()
                                                                    .text_xl()
                                                                    .text_color(white())
                                                                    .child(self.active_tab.label()),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_sm()
                                                                    .text_color(rgb(0x8E8E93))
                                                                    .child(
                                                                        self.active_tab.subtitle(),
                                                                    ),
                                                            ),
                                                    )
                                                    .child(if self.active_tab == MainTab::Docker {
                                                        Self::refresh_button(cx).into_any_element()
                                                    } else {
                                                        div().into_any_element()
                                                    }),
                                            )
                                            .child(div().p(px(20.0)).child(self.active_panel())),
                                    )
                                    .into_any_element(),
                            ),
                    )
                    .child(
                        div()
                            .px(px(20.0))
                            .py(px(10.0))
                            .border_t_1()
                            .border_color(rgb(0x3A3A3C))
                            .child(div().text_xs().text_color(rgb(0x8E8E93)).child(
                                "Machine sidebar on the left, resource tabs across the top",
                            )),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);

        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Crabdash".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(11.0))),
                    ..Default::default()
                }),
                ..WindowOptions::default()
            },
            |_window, cx| cx.new(|_cx| Crabdash::new()),
        )
        .expect("failed to open Crabdash window");
    });
}
