//! Resource histories use elapsed capture time, including gaps in sampling.
mod hover;
use super::ScalarPoint;
use crate::components::{common::clipped_text, style};
use gpui::{prelude::*, *};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

pub(super) struct Series {
    pub label: Option<&'static str>,
    pub samples: Vec<ScalarPoint>,
    pub color: u32,
    pub dashed: bool,
}

fn paint_is_visible(bounds: Bounds<Pixels>, mask: Bounds<Pixels>) -> bool {
    mask.size.width > px(0.0)
        && mask.size.height > px(0.0)
        && bounds.size.width > px(0.0)
        && bounds.size.height > px(0.0)
        // The 1.5px stroke can extend beyond the canvas; preserve its edge pixels.
        && bounds.dilate(px(1.0)).intersects(&mask)
}

fn fraction(captured_at: Instant, start: Instant, end: Instant) -> Option<f32> {
    let span = end.saturating_duration_since(start).as_secs_f64();
    (span > 0.0).then(|| {
        (captured_at.saturating_duration_since(start).as_secs_f64() / span).clamp(0.0, 1.0) as f32
    })
}

fn elapsed_label(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds == 0 {
        "−<1s".into()
    } else if seconds < 60 {
        format!("−{seconds}s")
    } else if seconds < 3600 {
        format!("−{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("−{}h {}m", seconds / 3600, seconds % 3600 / 60)
    }
}

fn known_value(sample: &ScalarPoint) -> Option<f64> {
    sample
        .value
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn scale_maximum(series: &[Series], fixed_max: Option<f64>) -> Option<f64> {
    fixed_max
        .filter(|value| value.is_finite() && *value > 0.0)
        .or_else(|| {
            series
                .iter()
                .flat_map(|series| &series.samples)
                .filter_map(known_value)
                .reduce(f64::max)
                // A floor gives known idle history a usable plotting range.
                // It is an axis limit, never a reported measurement or peak.
                .map(|value| value.max(1.0))
        })
}

fn scale_label(maximum: f64, format_maximum: fn(f64) -> String) -> String {
    format!("0–{}", format_maximum(maximum))
}

fn projected_segments(
    samples: &[ScalarPoint],
    start: Instant,
    end: Instant,
    maximum: f64,
) -> impl Iterator<Item = (Point<f32>, Point<f32>)> + '_ {
    let mut previous: Option<(Instant, Point<f32>)> = None;
    samples.iter().filter_map(move |sample| {
        let Some(value) = known_value(sample) else {
            previous = None;
            return None;
        };
        let x = fraction(sample.captured_at, start, end)?;
        let position = point(x, (value.clamp(0.0, maximum) / maximum) as f32);
        let segment = previous
            .filter(|(time, _)| sample.captured_at > *time)
            .map(|(_, from)| (from, position));
        previous = Some((sample.captured_at, position));
        segment
    })
}

fn segment(path: &mut PathBuilder, from: Point<Pixels>, to: Point<Pixels>, dashed: bool) {
    if !dashed {
        path.move_to(from);
        path.line_to(to);
        return;
    }
    let dx = f32::from(to.x - from.x);
    let dy = f32::from(to.y - from.y);
    let length = dx.hypot(dy);
    if !length.is_finite() || length <= 0.0 {
        return;
    }
    let mut offset = 0.0;
    while offset < length {
        let start = offset / length;
        let end = (offset + 5.0).min(length) / length;
        path.move_to(point(from.x + px(dx * start), from.y + px(dy * start)));
        path.line_to(point(from.x + px(dx * end), from.y + px(dy * end)));
        offset += 8.0;
    }
}

fn swatch(color: u32, dashed: bool) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            if !paint_is_visible(bounds, window.content_mask().bounds) {
                return;
            }
            let y = bounds.top() + bounds.size.height / 2.0;
            let mut path = PathBuilder::stroke(px(1.5));
            segment(
                &mut path,
                point(bounds.left(), y),
                point(bounds.right(), y),
                dashed,
            );
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(color));
            }
        },
    )
    .w(rems(18.0 / 16.0))
    .h(rems(style::META / 16.0))
    .flex_none()
}

pub(super) struct Settings {
    pub id: SharedString,
    pub caption: String,
    pub fixed_max: Option<f64>,
    pub format_maximum: fn(f64) -> String,
    pub format_value: fn(f64) -> String,
    pub inspect: bool,
}

pub(super) fn render(
    series: Vec<Series>,
    settings: Settings,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let Settings {
        id,
        caption,
        fixed_max,
        format_maximum,
        format_value,
        inspect,
    } = settings;
    let start = series
        .iter()
        .flat_map(|series| &series.samples)
        .map(|sample| sample.captured_at)
        .min();
    let end = series
        .iter()
        .flat_map(|series| &series.samples)
        .map(|sample| sample.captured_at)
        .max();
    let range = start.zip(end).filter(|(start, end)| end > start);
    let maximum = scale_maximum(&series, fixed_max);
    let has_known_values = series
        .iter()
        .flat_map(|series| &series.samples)
        .any(|sample| known_value(sample).is_some());
    let footer_range = range.filter(|_| has_known_values);
    let scale_text = footer_range
        .and(maximum)
        .map(|maximum| scale_label(maximum, format_maximum));
    let legend = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_wrap()
        .gap(rems(12.0 / 16.0))
        .children(series.iter().filter_map(|series| {
            series.label.map(|label| {
                div()
                    .flex()
                    .items_center()
                    .gap(rems(4.0 / 16.0))
                    .child(swatch(series.color, series.dashed))
                    .child(label)
            })
        }));
    let has_legend = series.iter().any(|series| series.label.is_some());
    let data = Rc::new(hover::Dataset {
        series,
        caption,
        format: format_value,
    });
    let readout = hover::state(id.clone(), data.clone(), inspect, window, cx);
    let measure = readout.clone();
    let plot = canvas(
        move |bounds, window, cx| hover::measure(&measure, bounds, inspect, window, cx),
        move |bounds, _, window, _| {
            if !paint_is_visible(bounds, window.content_mask().bounds) {
                return;
            }
            let Some(((start, end), maximum)) = range.zip(maximum) else {
                return;
            };
            for series in &data.series {
                let mut path = PathBuilder::stroke(px(1.5));
                let position = |value: Point<f32>| {
                    point(
                        bounds.left() + bounds.size.width * value.x,
                        bounds.bottom() - bounds.size.height * value.y,
                    )
                };
                for (from, to) in projected_segments(&series.samples, start, end, maximum) {
                    segment(&mut path, position(from), position(to), series.dashed);
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(series.color));
                }
            }
        },
    )
    .w_full()
    .h(rems(56.0 / 16.0))
    .flex_none();
    let plot = hover::inspect(plot, id, readout, inspect, cx);
    div()
        .w_full()
        .min_w_0()
        .flex_none()
        .flex()
        .flex_col()
        .gap(rems(4.0 / 16.0))
        .text_size(rems(style::META / 16.0))
        .text_color(rgb(style::TEXT_MUTED))
        .child(plot)
        .when(has_legend, |this| this.child(legend))
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .justify_between()
                .gap(rems(8.0 / 16.0))
                .when_some(footer_range, |this, (start, end)| {
                    this.child(elapsed_label(end.saturating_duration_since(start)))
                        .when_some(scale_text, |this, scale| this.child(clipped_text(scale)))
                        .child("Latest")
                })
                .when(footer_range.is_none(), |this| {
                    this.child("Collecting history…")
                }),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        ScalarPoint, Series, elapsed_label, fraction, paint_is_visible, projected_segments,
        scale_label, scale_maximum,
    };
    use gpui::{Bounds, point, px, size};
    use std::time::{Duration, Instant};

    #[test]
    fn irregular_samples_use_elapsed_time_instead_of_sample_index() {
        let start = Instant::now();
        let end = start + Duration::from_secs(10);
        assert_eq!(fraction(start, start, end), Some(0.0));
        assert_eq!(
            fraction(start + Duration::from_secs(2), start, end),
            Some(0.2)
        );
        assert_eq!(fraction(end, start, end), Some(1.0));
        assert_eq!(fraction(start, start, start), None);
        assert_eq!(elapsed_label(Duration::from_secs(102)), "−1m 42s");
    }

    #[test]
    fn missing_invalid_and_duplicate_samples_do_not_bridge_gaps() {
        let start = Instant::now();
        let end = start + Duration::from_secs(9);
        let sample = |seconds, value| ScalarPoint {
            captured_at: start + Duration::from_secs(seconds),
            value,
        };
        for missing in [None, Some(f64::NAN), Some(f64::INFINITY), Some(-1.0)] {
            let samples = [
                sample(0, Some(0.0)),
                sample(1, missing),
                sample(9, Some(50.0)),
            ];
            assert_eq!(projected_segments(&samples, start, end, 100.0).count(), 0);
        }
        let samples = [
            sample(0, Some(0.0)),
            sample(1, Some(10.0)),
            sample(1, Some(20.0)),
            sample(9, Some(50.0)),
        ];
        let segments: Vec<_> = projected_segments(&samples, start, end, 100.0).collect();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].1.x, 1.0 / 9.0);
        assert_eq!(segments[1].0.y, 0.2);
        assert_eq!(segments[1].1.x, 1.0);
        assert_eq!(projected_segments(&samples, start, start, 100.0).count(), 0);
    }

    fn series(values: &[Option<f64>]) -> Series {
        let start = Instant::now();
        Series {
            label: None,
            samples: values
                .iter()
                .enumerate()
                .map(|(index, value)| ScalarPoint {
                    captured_at: start + Duration::from_secs(index as u64),
                    value: *value,
                })
                .collect(),
            color: 0,
            dashed: false,
        }
    }

    #[test]
    fn autoscale_uses_known_values_from_both_series_and_retains_idle_floor() {
        assert_eq!(
            scale_maximum(&[series(&[Some(10.0)]), series(&[Some(95.6)])], None),
            Some(95.6)
        );
        assert_eq!(
            scale_maximum(&[series(&[Some(95.6)]), series(&[Some(10.0)])], None),
            Some(95.6)
        );
        assert_eq!(
            scale_maximum(&[series(&[Some(0.0), Some(0.0)])], None),
            Some(1.0)
        );
        assert_eq!(scale_maximum(&[series(&[Some(0.5)])], None), Some(1.0));
        assert_eq!(scale_maximum(&[], None), None);
        assert_eq!(
            scale_maximum(
                &[series(&[
                    None,
                    Some(f64::NAN),
                    Some(f64::INFINITY),
                    Some(-10.0)
                ])],
                None
            ),
            None
        );
        assert_eq!(
            scale_maximum(&[series(&[Some(-100.0), Some(42.0)])], None),
            Some(42.0)
        );
    }

    #[test]
    fn fixed_scale_is_known_independently_but_invalid_limits_use_autoscale() {
        assert_eq!(scale_maximum(&[], Some(100.0)), Some(100.0));
        assert_eq!(
            scale_maximum(&[series(&[Some(150.0)])], Some(100.0)),
            Some(100.0)
        );
        for maximum in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(scale_maximum(&[], Some(maximum)), None);
            assert_eq!(
                scale_maximum(&[series(&[Some(42.0)])], Some(maximum)),
                Some(42.0)
            );
        }
        assert_eq!(scale_label(100.0, |value| format!("{value:.0}%")), "0–100%");
    }

    #[test]
    fn axis_label_and_projection_share_the_selected_maximum() -> anyhow::Result<()> {
        let history = series(&[Some(0.0), Some(24.0), Some(96.0)]);
        let histories = [history];
        let maximum = scale_maximum(&histories, None)
            .ok_or_else(|| anyhow::anyhow!("Known history must have a scale"))?;
        assert_eq!(
            scale_label(maximum, |value| format!("{value:.1} B/s")),
            "0–96.0 B/s"
        );
        let samples = &histories[0].samples;
        let segments: Vec<_> = projected_segments(
            samples,
            samples[0].captured_at,
            samples[2].captured_at,
            maximum,
        )
        .collect();
        assert_eq!(segments[0].1.y, 0.25);
        assert_eq!(segments[1].1.y, 1.0);
        Ok(())
    }

    #[test]
    fn visibility_respects_translated_masks_stroke_edges_and_empty_clips() {
        let mask = Bounds::new(point(px(100.0), px(100.0)), size(px(200.0), px(200.0)));
        let canvas = |y| Bounds::new(point(px(150.0), px(y)), size(px(56.0), px(56.0)));
        assert!(paint_is_visible(canvas(250.0), mask));
        assert!(paint_is_visible(canvas(300.5), mask));
        assert!(!paint_is_visible(canvas(301.0), mask));
        let empty = Bounds::new(point(px(150.0), px(150.0)), size(px(0.0), px(0.0)));
        assert!(!paint_is_visible(canvas(150.0), empty));
    }
}
