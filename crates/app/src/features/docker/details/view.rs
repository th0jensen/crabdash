use super::{Entry, Key};
use crate::{
    app::Crabdash,
    components::{
        common::{clipped_text, control_tooltip, lucide_icon},
        style,
    },
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use utils::container::details::{
    ContainerMount, ContainerPort, ContainerRestartPolicy, PortBinding,
};

pub(crate) fn render(app: &Crabdash, key: &Key, compact: bool, cx: &mut Context<Crabdash>) -> Div {
    let entry = app.docker_details.get(key);
    let target_valid = app
        .machine_store
        .machines
        .iter()
        .find(|machine| machine.uuid == key.0)
        .is_some_and(|machine| app.docker_details.matches_target(key, machine));
    let loading = matches!(entry, Some(Entry::Loading));
    let refresh_key = key.clone();
    let mut panel = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(rems(8.0 / 16.0))
        .px(rems(12.0 / 16.0))
        .py(rems(10.0 / 16.0))
        .bg(rgb(style::SURFACE))
        .border_b_1()
        .border_color(rgb(style::BORDER))
        .text_size(rems(style::META / 16.0))
        .text_color(rgb(style::TEXT_PRIMARY))
        .child(
            div()
                .flex()
                .items_center()
                .gap(rems(6.0 / 16.0))
                .h(rems(style::CONTROL / 16.0))
                .text_color(rgb(style::TEXT_MUTED))
                .child(lucide_icon(Icon::Info, style::ICON))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child("Container details · snapshot"),
                )
                .when(!loading, |header| {
                    header.child(
                        div()
                            .id(SharedString::from(format!(
                                "{}-{}-details-refresh",
                                key.0, key.1
                            )))
                            .size(rems(style::CONTROL / 16.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(style::RADIUS))
                            .cursor_pointer()
                            .hover(|style| {
                                style
                                    .bg(rgb(style::SURFACE_HOVER))
                                    .text_color(rgb(style::TEXT_SELECTED))
                            })
                            .tooltip(|_, cx| control_tooltip("Refresh container details", cx))
                            .child(lucide_icon(Icon::RefreshCw, style::ICON))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.refresh_docker_details(refresh_key.clone(), cx)
                            })),
                    )
                }),
        );
    if !target_valid {
        return panel.child(
            div()
                .text_color(rgb(style::TEXT_MUTED))
                .child("Connection changed. Refresh to load current details."),
        );
    }
    match entry {
        Some(Entry::Loading) => panel.child(
            div()
                .text_color(rgb(style::TEXT_MUTED))
                .child("Loading container details…"),
        ),
        Some(Entry::Error(message)) => panel.child(
            div()
                .flex()
                .flex_col()
                .gap(rems(8.0 / 16.0))
                .child(div().text_color(rgb(style::DANGER)).child(message.clone()))
                .child(
                    div()
                        .text_color(rgb(style::TEXT_MUTED))
                        .child("Use Refresh to retry."),
                ),
        ),
        Some(Entry::Ready(details)) => {
            panel = panel
                .child(copy_row(
                    "ID",
                    &details.id,
                    compact,
                    format!("{}-{}-details-id", key.0, key.1),
                ))
                .child(match &details.image {
                    Some(image) => copy_row(
                        "Image",
                        image,
                        compact,
                        format!("{}-{}-details-image", key.0, key.1),
                    ),
                    None => row("Image", "Unavailable".into(), compact),
                })
                .child(row(
                    "State",
                    details
                        .status
                        .clone()
                        .unwrap_or_else(|| "Unavailable".into()),
                    compact,
                ))
                .child(row(
                    "Health",
                    details
                        .health
                        .clone()
                        .unwrap_or_else(|| "Not reported".into()),
                    compact,
                ))
                .child(row(
                    "Restart",
                    details
                        .restart_policy
                        .as_ref()
                        .map(restart_policy)
                        .unwrap_or_else(|| "Unavailable".into()),
                    compact,
                ))
                .child(row(
                    "Ports",
                    details
                        .ports
                        .as_ref()
                        .map(|ports| {
                            if ports.is_empty() {
                                "No exposed ports".into()
                            } else {
                                ports.iter().map(format_port).collect::<Vec<_>>().join("\n")
                            }
                        })
                        .unwrap_or_else(|| "Unavailable".into()),
                    compact,
                ))
                .child(row(
                    "Mounts",
                    details
                        .mounts
                        .as_ref()
                        .map(|mounts| {
                            if mounts.is_empty() {
                                "No mounts".into()
                            } else {
                                mounts
                                    .iter()
                                    .map(format_mount)
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            }
                        })
                        .unwrap_or_else(|| "Unavailable".into()),
                    compact,
                ))
                .child(row(
                    "Created",
                    details
                        .created
                        .clone()
                        .unwrap_or_else(|| "Unavailable".into()),
                    compact,
                ));
            panel
        }
        None => panel,
    }
}

fn row(label: &'static str, value: String, compact: bool) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .gap(rems(4.0 / 16.0))
        .when(compact, |row| row.flex_col())
        .child(
            div()
                .flex_none()
                .text_color(rgb(style::TEXT_MUTED))
                .when(!compact, |label| label.w(rems(80.0 / 16.0)))
                .child(label),
        )
        .child(div().flex_1().min_w_0().child(value))
}

fn copy_row(label: &'static str, value: &str, compact: bool, id: String) -> Div {
    let text = value.to_string();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .gap(rems(4.0 / 16.0))
        .when(compact, |row| row.flex_col())
        .child(
            div()
                .flex_none()
                .text_color(rgb(style::TEXT_MUTED))
                .when(!compact, |label| label.w(rems(80.0 / 16.0)))
                .child(label),
        )
        .child(
            div()
                .id(SharedString::from(id))
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(rems(6.0 / 16.0))
                .cursor_pointer()
                .hover(|style| style.text_color(rgb(style::TEXT_SELECTED)))
                .tooltip(move |_, cx| control_tooltip(format!("Copy {}", label.to_lowercase()), cx))
                .child(clipped_text(value.to_string()).flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(style::TEXT_MUTED))
                        .child(lucide_icon(Icon::Copy, style::ICON)),
                )
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                }),
        )
}

fn restart_policy(policy: &ContainerRestartPolicy) -> String {
    match policy.name.as_str() {
        "no" => "Never".into(),
        "always" => "Always".into(),
        "unless-stopped" => "Unless stopped".into(),
        "on-failure" => match policy.maximum_retry_count {
            Some(0) => "On failure · unlimited retries".into(),
            Some(count) => format!(
                "On failure · {count} {}",
                if count == 1 { "retry" } else { "retries" }
            ),
            None => "On failure · retry limit unavailable".into(),
        },
        other => other.to_string(),
    }
}

fn format_binding(binding: &PortBinding) -> String {
    let address = if binding.host_ip.is_empty() {
        "*"
    } else {
        binding.host_ip.as_str()
    };
    if address.contains(':') && !address.starts_with('[') {
        format!("[{address}]:{}", binding.host_port)
    } else {
        format!("{address}:{}", binding.host_port)
    }
}
fn format_port(port: &ContainerPort) -> String {
    let container = format!("{}/{}", port.container_port, port.protocol);
    match &port.bindings {
        None => format!("{container} · bindings unavailable"),
        Some(bindings) if bindings.is_empty() => format!("{container} · unpublished"),
        Some(bindings) => format!(
            "{} → {container}",
            bindings
                .iter()
                .map(format_binding)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
fn format_mount(mount: &ContainerMount) -> String {
    let kind = mount.mount_type.as_deref().unwrap_or("type unavailable");
    let source = mount
        .name
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| mount.source.as_deref().filter(|value| !value.is_empty()));
    let destination = mount
        .destination
        .as_deref()
        .unwrap_or("destination unavailable");
    let path = source.map_or_else(
        || destination.to_string(),
        |source| format!("{source} → {destination}"),
    );
    let access = match mount.read_write {
        Some(true) => "read/write",
        Some(false) => "read-only",
        None => "access unavailable",
    };
    let mode = mount
        .mode
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .filter(|value| {
            !value.is_empty()
                && !matches!(
                    (mount.read_write, *value),
                    (Some(true), "rw") | (Some(false), "ro")
                )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{path} · {kind} · {access}{}",
        if mode.is_empty() {
            String::new()
        } else {
            format!(" · {mode}")
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::test;

    #[test]
    fn formats_every_binding_and_preserves_unavailable_vs_unpublished() {
        let mut port = ContainerPort {
            container_port: 80,
            protocol: "tcp".into(),
            bindings: Some(vec![
                PortBinding {
                    host_ip: "0.0.0.0".into(),
                    host_port: 8080,
                },
                PortBinding {
                    host_ip: "::".into(),
                    host_port: 8080,
                },
            ]),
        };
        assert_eq!(format_port(&port), "0.0.0.0:8080, [::]:8080 → 80/tcp");
        port.bindings = Some(Vec::new());
        assert_eq!(format_port(&port), "80/tcp · unpublished");
        port.bindings = None;
        assert_eq!(format_port(&port), "80/tcp · bindings unavailable");
        assert_eq!(
            format_binding(&PortBinding {
                host_ip: "".into(),
                host_port: 80
            }),
            "*:80"
        );
    }

    #[test]
    fn mount_format_handles_tmpfs_read_only_and_missing_access() {
        let mut mount = ContainerMount {
            name: None,
            mount_type: Some("tmpfs".into()),
            source: Some(String::new()),
            destination: Some("/tmp".into()),
            mode: None,
            read_write: Some(false),
        };
        assert_eq!(format_mount(&mount), "/tmp · tmpfs · read-only");
        mount.mode = Some("ro,z".into());
        assert_eq!(format_mount(&mount), "/tmp · tmpfs · read-only · z");
        mount.mode = Some("rw".into());
        assert_eq!(format_mount(&mount), "/tmp · tmpfs · read-only · rw");
        mount.source = Some("/srv/data".into());
        mount.read_write = None;
        mount.mode = Some("z".into());
        assert_eq!(
            format_mount(&mount),
            "/srv/data → /tmp · tmpfs · access unavailable · z"
        );
        mount.name = Some("cache".into());
        mount.mount_type = Some("volume".into());
        assert_eq!(
            format_mount(&mount),
            "cache → /tmp · volume · access unavailable · z"
        );
    }

    #[test]
    fn on_failure_zero_retries_means_unlimited() {
        assert_eq!(
            restart_policy(&ContainerRestartPolicy {
                name: "on-failure".into(),
                maximum_retry_count: Some(0)
            }),
            "On failure · unlimited retries"
        );
    }
}
