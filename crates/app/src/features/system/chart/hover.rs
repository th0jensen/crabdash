//! Inspect captured samples without interpolation or collapsing missing intervals.
use super::{Series, elapsed_label, known_value};
use crate::components::common::tooltip_text;
use gpui::{
    App, Bounds, Context, Div, Entity, Pixels, Point, Render, SharedString, Stateful, Window, div,
    prelude::*, px, rems,
};
use std::{rc::Rc, time::Instant};

#[derive(Debug, PartialEq)]
struct Selection {
    captured_at: Instant,
    values: Vec<Option<f64>>,
}

fn select(series: &[Series], fraction: f64) -> Option<Selection> {
    if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
        return None;
    }
    let start = series
        .iter()
        .flat_map(|series| &series.samples)
        .map(|point| point.captured_at)
        .min()?;
    let end = series
        .iter()
        .flat_map(|series| &series.samples)
        .map(|point| point.captured_at)
        .max()?;
    let target = end.saturating_duration_since(start).as_secs_f64() * fraction;
    let captured_at = series
        .iter()
        .flat_map(|series| &series.samples)
        .map(|point| point.captured_at)
        .min_by(|a, b| {
            let distance = |time: Instant| {
                (time.saturating_duration_since(start).as_secs_f64() - target).abs()
            };
            distance(*a).total_cmp(&distance(*b)).then_with(|| a.cmp(b))
        })?;
    Some(Selection {
        captured_at,
        values: series
            .iter()
            .map(|series| {
                series
                    .samples
                    .iter()
                    .find(|point| point.captured_at == captured_at)
                    .and_then(known_value)
            })
            .collect(),
    })
}

#[derive(Clone, Copy, PartialEq)]
struct Plot {
    bounds: Bounds<Pixels>,
    mask: Bounds<Pixels>,
}

fn fraction(plot: Plot, position: Point<Pixels>) -> Option<f64> {
    let bounds = plot.bounds;
    let visible = bounds.intersect(&plot.mask);
    let finite = [
        bounds.left(),
        bounds.top(),
        bounds.size.width,
        bounds.size.height,
        plot.mask.left(),
        plot.mask.top(),
        plot.mask.size.width,
        plot.mask.size.height,
        position.x,
        position.y,
    ]
    .into_iter()
    .all(|value| f32::from(value).is_finite());
    if !finite
        || bounds.size.width <= px(0.0)
        || bounds.size.height <= px(0.0)
        || visible.size.width <= px(0.0)
        || visible.size.height <= px(0.0)
        || !visible.contains(&position)
    {
        return None;
    }
    Some(f64::from(f32::from(position.x - bounds.left())) / f64::from(f32::from(bounds.size.width)))
}

pub(super) struct Dataset {
    pub series: Vec<Series>,
    pub caption: String,
    pub format: fn(f64) -> String,
}

impl Dataset {
    fn matches(&self, other: &Self) -> bool {
        self.caption == other.caption
            && self.series.len() == other.series.len()
            && self.series.iter().zip(&other.series).all(|(a, b)| {
                a.label == b.label
                    && a.samples.len() == b.samples.len()
                    && a.samples.iter().zip(&b.samples).all(|(a, b)| {
                        a.captured_at == b.captured_at && known_value(a) == known_value(b)
                    })
            })
    }
}

pub(super) struct Readout {
    data: Rc<Dataset>,
    plot: Option<Plot>,
    pointer: Option<Point<Pixels>>,
    text: Option<String>,
}

impl Readout {
    fn new(data: Rc<Dataset>) -> Self {
        Self {
            data,
            plot: None,
            pointer: None,
            text: None,
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let selection = self
            .plot
            .zip(self.pointer)
            .and_then(|(plot, pointer)| fraction(plot, pointer))
            .and_then(|fraction| select(&self.data.series, fraction));
        let text = selection.map(|selection| {
            let latest = self
                .data
                .series
                .iter()
                .flat_map(|series| &series.samples)
                .map(|point| point.captured_at)
                .max()
                .unwrap_or(selection.captured_at);
            let age = latest.saturating_duration_since(selection.captured_at);
            let time = if age.is_zero() {
                "Latest".into()
            } else {
                elapsed_label(age)
            };
            let mut fields = vec![format!("{} · {time}", self.data.caption)];
            for (series, value) in self.data.series.iter().zip(selection.values) {
                let value = value.map_or_else(|| "Unavailable".into(), self.data.format);
                fields.push(
                    series
                        .label
                        .map_or(value.clone(), |label| format!("{label}: {value}")),
                );
            }
            fields.join(" · ")
        });
        if self.text != text {
            self.text = text;
            cx.notify();
        }
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.pointer = None;
        if self.text.take().is_some() {
            cx.notify();
        }
    }
}

impl Render for Readout {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.text.as_ref().map_or_else(
            || div().into_any_element(),
            |text| tooltip_text(text.clone()).into_any_element(),
        )
    }
}

pub(super) fn state(
    id: SharedString,
    data: Rc<Dataset>,
    enabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Readout> {
    let initial = data.clone();
    let state = window.use_keyed_state(id, cx, |_, _| Readout::new(initial));
    let dragging = cx.has_active_drag();
    state.update(cx, |state, cx| {
        let changed = !state.data.matches(&data);
        state.data = data;
        if !enabled || dragging {
            state.clear(cx);
        } else if changed {
            state.refresh(cx);
        }
    });
    state
}

pub(super) fn measure(
    state: &Entity<Readout>,
    bounds: Bounds<Pixels>,
    enabled: bool,
    window: &Window,
    cx: &mut App,
) {
    let dragging = cx.has_active_drag();
    state.update(cx, |state, cx| {
        let plot = Some(Plot {
            bounds,
            mask: window.content_mask().bounds,
        });
        let changed = state.plot != plot;
        state.plot = plot;
        if !enabled || dragging {
            state.clear(cx);
        } else if changed {
            state.refresh(cx);
        }
    });
}

pub(super) fn inspect(
    plot: impl IntoElement,
    id: SharedString,
    state: Entity<Readout>,
    enabled: bool,
    cx: &App,
) -> Stateful<Div> {
    let enabled = enabled
        && !cx.has_active_drag()
        && state
            .read(cx)
            .data
            .series
            .iter()
            .any(|series| !series.samples.is_empty());
    let movement = state.clone();
    let leave = state.clone();
    div()
        .id(id)
        .w_full()
        .h(rems(56.0 / 16.0))
        .flex_none()
        .child(plot)
        .on_mouse_move(move |event, _, cx| {
            let dragging = cx.has_active_drag();
            movement.update(cx, |state, cx| {
                if !enabled || dragging {
                    state.clear(cx);
                } else {
                    state.pointer = Some(event.position);
                    state.refresh(cx);
                }
            });
        })
        .on_hover(move |hovered, window, cx| {
            let dragging = cx.has_active_drag();
            leave.update(cx, |state, cx| {
                if !hovered || !enabled || dragging {
                    state.clear(cx);
                } else {
                    state.pointer = Some(window.mouse_position());
                    state.refresh(cx);
                }
            });
        })
        .when(enabled, |plot| {
            plot.tooltip(move |_, _| state.clone().into())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::system::ScalarPoint;
    use gpui::{point, size};
    use std::time::Duration;

    fn series(start: Instant, values: &[(u64, Option<f64>)]) -> Series {
        Series {
            label: None,
            samples: values
                .iter()
                .map(|(seconds, value)| ScalarPoint {
                    captured_at: start + Duration::from_secs(*seconds),
                    value: *value,
                })
                .collect(),
            color: 0,
            dashed: false,
        }
    }

    #[test]
    fn selection_uses_elapsed_union_captures_including_gaps() {
        let start = Instant::now();
        let data = [
            series(start, &[(0, Some(1.0)), (100, Some(3.0))]),
            series(start, &[(2, None)]),
        ];
        let selected = select(&data, 0.03).expect("capture");
        assert_eq!(selected.captured_at, start + Duration::from_secs(2));
        assert_eq!(selected.values, vec![None, None]);
        assert_eq!(
            select(&data, 0.6).expect("capture").captured_at,
            start + Duration::from_secs(100)
        );
    }

    #[test]
    fn values_require_exact_timestamps_and_valid_measurements() {
        let start = Instant::now();
        for missing in [None, Some(f64::NAN), Some(f64::INFINITY), Some(-1.0)] {
            let data = [
                series(start, &[(0, Some(0.0)), (2, missing)]),
                series(start, &[(0, Some(100.0)), (1, Some(5.0))]),
            ];
            assert_eq!(
                select(&data, 1.0).expect("capture").values,
                vec![None, None]
            );
            assert_eq!(
                select(&data, 0.0).expect("capture").values,
                vec![Some(0.0), Some(100.0)]
            );
        }
    }

    #[test]
    fn ties_choose_earlier_capture_independent_of_provider_order() {
        let start = Instant::now();
        for values in [
            vec![(2, Some(2.0)), (0, Some(0.0))],
            vec![(0, Some(0.0)), (2, Some(2.0))],
        ] {
            let data = [series(start, &values)];
            assert_eq!(select(&data, 0.5).expect("capture").captured_at, start);
        }
        let single = [series(start, &[(2, Some(3.0))])];
        assert_eq!(
            select(&single, 0.7).expect("capture").values,
            vec![Some(3.0)]
        );
        assert!(select(&[], 0.0).is_none());
        for invalid in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            assert!(select(&single, invalid).is_none());
        }
    }

    #[test]
    fn hit_testing_respects_clip_but_maps_the_original_plot() {
        let bounds = Bounds::new(point(px(100.0), px(100.0)), size(px(200.0), px(56.0)));
        let mask = Bounds::new(point(px(150.0), px(110.0)), size(px(100.0), px(20.0)));
        assert_eq!(
            fraction(Plot { bounds, mask }, point(px(150.0), px(120.0))),
            Some(0.25)
        );
        assert_eq!(
            fraction(Plot { bounds, mask }, point(px(200.0), px(120.0))),
            Some(0.5)
        );
        for position in [
            point(px(125.0), px(120.0)),
            point(px(200.0), px(100.0)),
            point(px(f32::NAN), px(120.0)),
            point(px(f32::INFINITY), px(120.0)),
        ] {
            assert!(fraction(Plot { bounds, mask }, position).is_none());
        }
        for mask in [
            Bounds::new(point(px(400.0), px(110.0)), size(px(100.0), px(20.0))),
            Bounds::new(point(px(150.0), px(110.0)), size(px(0.0), px(20.0))),
        ] {
            assert!(fraction(Plot { bounds, mask }, point(px(200.0), px(120.0))).is_none());
        }
        for dimensions in [size(px(0.0), px(56.0)), size(px(200.0), px(0.0))] {
            let zero = Bounds::new(bounds.origin, dimensions);
            assert!(fraction(Plot { bounds: zero, mask }, point(px(200.0), px(120.0))).is_none());
        }
    }
}
