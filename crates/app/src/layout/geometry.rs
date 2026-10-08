//! Shared edge bands and anchored splitter geometry.
use super::Drop;
use gpui::{Bounds, Pixels, Point, px};

pub(crate) fn target(
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
    center: usize,
) -> Option<Drop> {
    if !bounds.contains(&position) {
        return None;
    }
    let x = f32::from(position.x - bounds.origin.x);
    let y = f32::from(position.y - bounds.origin.y);
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let band = width.min(height) * 0.2;
    let edges = [
        (y, Drop::Top),
        (width - x, Drop::Right),
        (height - y, Drop::Bottom),
        (x, Drop::Left),
    ];
    let mut nearest = edges[0];
    for edge in edges.iter().skip(1) {
        if edge.0 < nearest.0 {
            nearest = *edge;
        }
    }
    Some(if nearest.0 < band {
        nearest.1
    } else {
        Drop::Tab(center)
    })
}

pub(crate) fn usable_ratio(ratio: f32, extent: f32, first_min: f32, second_min: f32) -> f32 {
    let available = (extent - 1.0).max(0.0);
    if available >= first_min + second_min {
        ratio.clamp(first_min / available, 1.0 - second_min / available)
    } else {
        first_min / (first_min + second_min)
    }
}

pub(crate) fn drag_ratio(
    pointer: f32,
    origin: f32,
    extent: f32,
    grab: f32,
    first_min: f32,
    second_min: f32,
) -> Option<f32> {
    let fraction = (pointer - origin - grab - 0.5) / (extent - 1.0);
    (extent > 1.0 && fraction.is_finite())
        .then(|| usable_ratio(fraction, extent, first_min, second_min))
}

pub(crate) fn reveal_offset(
    current: Pixels,
    width: Pixels,
    viewport: Pixels,
    index: usize,
) -> Pixels {
    if viewport <= px(0.0) {
        return current;
    }
    let left = width * index as f32;
    let right = left + width;
    if left + current < px(0.0) || width > viewport {
        -left
    } else if right + current > viewport {
        viewport - right
    } else {
        current
    }
}
