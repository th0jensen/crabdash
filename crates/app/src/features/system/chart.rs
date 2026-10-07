//! Resource histories use elapsed capture time, including gaps in sampling.
use super::ScalarPoint;
use crate::components::style;
use gpui::{prelude::*, *};
use std::time::{Duration, Instant};

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

fn projected_segments(
    samples: &[ScalarPoint],
    start: Instant,
    end: Instant,
    maximum: f64,
) -> impl Iterator<Item = (Point<f32>, Point<f32>)> + '_ {
    let mut previous: Option<(Instant, Point<f32>)> = None;
    samples.iter().filter_map(move |sample| {
        let Some(value) = sample.value.filter(|value| value.is_finite()) else {
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

pub(super) fn render(series: Vec<Series>, fixed_max: Option<f64>) -> Div {
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
    let maximum = fixed_max
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or_else(|| {
            series
                .iter()
                .flat_map(|series| &series.samples)
                .filter_map(|sample| sample.value)
                .filter(|value| value.is_finite())
                .fold(1.0_f64, f64::max)
        });
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
    let plot = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            if !paint_is_visible(bounds, window.content_mask().bounds) {
                return;
            }
            let Some((start, end)) = range else {
                return;
            };
            for series in &series {
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
                .flex()
                .justify_between()
                .when_some(range, |this, (start, end)| {
                    this.child(elapsed_label(end.saturating_duration_since(start)))
                        .child("Latest")
                })
                .when(range.is_none(), |this| this.child("Collecting history…")),
        )
}

#[cfg(test)]
mod tests {
    use super::{ScalarPoint, elapsed_label, fraction, paint_is_visible, projected_segments};
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
        for missing in [None, Some(f64::NAN), Some(f64::INFINITY)] {
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
