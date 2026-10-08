//! Per-processor disclosure retains provider order and each machine's choice.
use super::view::meter;
use crate::{
    app::Crabdash,
    components::{
        common::{clipped_text, control_tooltip, surface_button},
        style,
    },
};
use gpui::{prelude::*, *};
use machines::resources::CpuUsage;
use std::collections::HashSet;
use uuid::Uuid;

const PREVIEW_LIMIT: usize = 8;

#[derive(Default)]
pub(super) struct State {
    expanded: HashSet<Uuid>,
}

impl State {
    pub(super) fn expanded(&self, machine: Uuid) -> bool {
        self.expanded.contains(&machine)
    }

    fn toggle(&mut self, machine: Uuid) {
        if !self.expanded.remove(&machine) {
            self.expanded.insert(machine);
        }
    }

    pub(super) fn remove(&mut self, machine: Uuid) {
        self.expanded.remove(&machine);
    }
}

fn visible(cores: &[CpuUsage], expanded: bool) -> &[CpuUsage] {
    &cores[..if expanded {
        cores.len()
    } else {
        cores.len().min(PREVIEW_LIMIT)
    }]
}

fn reading(value: Option<f64>) -> (String, Option<f64>) {
    let value = value.filter(|value| value.is_finite());
    (
        value.map_or_else(|| "—".into(), |value| format!("{value:.1}%")),
        value,
    )
}

pub(super) fn card_width(pane_width: f32, mosaic_columns: u16) -> f32 {
    let span: u16 = if mosaic_columns == 3 { 2 } else { 1 };
    let column_width =
        (pane_width - 12.0 * f32::from(mosaic_columns - 1)) / f32::from(mosaic_columns);
    column_width * f32::from(span) + 12.0 * f32::from(span - 1)
}

fn grid_columns(card_width: f32, core_count: usize) -> u16 {
    let inner_width = card_width - 24.0;
    (((inner_width + 10.0) / (104.0 + 10.0)).floor().max(1.0) as u16)
        .min(core_count.clamp(1, u16::MAX as usize) as u16)
}

pub(super) fn render(
    app: &Crabdash,
    cores: &[CpuUsage],
    card_width: f32,
    accent: Option<u32>,
    cx: &mut Context<Crabdash>,
) -> Div {
    let machine = app.selected_machine().uuid;
    let expanded = app.system.cores.expanded(machine);
    let section = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(rems(10.0 / 16.0))
        .text_size(rems(style::META / 16.0));
    if cores.is_empty() {
        return section
            .text_color(rgb(style::TEXT_MUTED))
            .child("Per-processor usage unavailable");
    }
    let shown = visible(cores, expanded);
    section
        .when(cores.len() > PREVIEW_LIMIT, |section| {
            let label = if expanded {
                "Show fewer".to_string()
            } else {
                format!("Show all {}", cores.len())
            };
            let tooltip = if expanded {
                format!("Show the first {PREVIEW_LIMIT} logical processors")
            } else {
                format!("Show all {} logical processors", cores.len())
            };
            section.child(
                div().flex().child(
                    surface_button(
                        SharedString::from(format!("system-cores-{machine}")),
                        None,
                        Some(&label),
                    )
                    .tooltip(move |_, cx| control_tooltip(tooltip.clone(), cx))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if app.selected_machine().uuid == machine {
                            app.system.cores.toggle(machine);
                            cx.notify();
                        }
                    })),
                ),
            )
        })
        .child(
            div()
                .w_full()
                .min_w_0()
                .grid()
                .grid_cols(grid_columns(card_width, shown.len()))
                .gap(rems(10.0 / 16.0))
                .children(shown.iter().map(|core| {
                    let label = format!("CPU {}", core.name);
                    let (value_label, value) = reading(core.percent);
                    let tooltip = format!(
                        "{label}: {}",
                        if value.is_some() {
                            value_label.as_str()
                        } else {
                            "Sampling per-processor usage…"
                        }
                    );
                    div()
                        .id(SharedString::from(format!(
                            "system-core-{machine}-{}",
                            core.name
                        )))
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .tooltip(move |_, cx| control_tooltip(tooltip.clone(), cx))
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .flex()
                                .justify_between()
                                .gap(px(4.0))
                                .child(
                                    clipped_text(label)
                                        .flex_1()
                                        .text_color(rgb(style::TEXT_MUTED)),
                                )
                                .child(div().flex_none().child(value_label)),
                        )
                        .child(meter(value.unwrap_or(0.0), accent))
                })),
        )
}

#[cfg(test)]
mod tests {
    use super::{State, card_width, grid_columns, reading, visible};
    use machines::resources::CpuUsage;
    use uuid::Uuid;

    #[test]
    fn preview_is_bounded_and_preserves_provider_names_and_order() {
        for count in [0, 8, 9, 64] {
            let cores: Vec<_> = (0..count)
                .map(|index| CpuUsage {
                    name: format!("provider-{}", count - index),
                    percent: Some(index as f64),
                })
                .collect();
            for expanded in [false, true] {
                let shown = visible(&cores, expanded);
                assert_eq!(shown.len(), if expanded { count } else { count.min(8) });
                for (actual, expected) in shown.iter().zip(&cores) {
                    assert_eq!(actual.name, expected.name);
                    assert_eq!(actual.percent, expected.percent);
                }
            }
        }
    }

    #[test]
    fn expansion_and_removal_are_independent_per_machine() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut state = State::default();
        assert!(!state.expanded(first));
        state.toggle(first);
        assert!(state.expanded(first));
        assert!(!state.expanded(second));
        state.toggle(second);
        state.toggle(first);
        assert!(!state.expanded(first));
        assert!(state.expanded(second));
        state.toggle(first);
        state.remove(first);
        assert!(!state.expanded(first));
        assert!(state.expanded(second));
    }

    #[test]
    fn missing_readings_have_no_fill_and_zero_remains_a_measurement() {
        for value in [
            None,
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(f64::NEG_INFINITY),
        ] {
            assert_eq!(reading(value), ("—".into(), None));
        }
        assert_eq!(reading(Some(0.0)), ("0.0%".into(), Some(0.0)));
        assert_eq!(reading(Some(100.0)), ("100.0%".into(), Some(100.0)));
    }

    #[test]
    fn responsive_columns_fill_the_actual_cpu_card_span() {
        assert_eq!(grid_columns(200.0, 64), 1);
        assert_eq!(grid_columns(300.0, 64), 2);
        assert_eq!(grid_columns(720.0, 64), 6);
        assert_eq!(grid_columns(1400.0, 6), 6);
        assert_eq!(grid_columns(1400.0, 8), 8);
        assert_eq!(grid_columns(1400.0, 0), 1);
        assert_eq!(card_width(200.0, 1), 200.0);
        let two_columns = card_width(600.0, 2);
        assert_eq!(two_columns, 294.0);
        assert_eq!(grid_columns(two_columns, 64), 2);
        let three_columns = card_width(1000.0, 3);
        assert!((three_columns - (976.0 / 3.0 * 2.0 + 12.0)).abs() < 0.001);
        assert_eq!(grid_columns(three_columns, 64), 5);
    }
}
