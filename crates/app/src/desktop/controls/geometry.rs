//! Clip native hosts while preserving the full control's local coordinates.
use gpui::{Bounds, Pixels, Point, point};

pub(super) fn content_origin(bounds: Bounds<Pixels>, visible: Bounds<Pixels>) -> Point<Pixels> {
    // AppKit hosts use bottom-left coordinates, unlike GPUI's top-left layout.
    point(
        visible.left() - bounds.left(),
        bounds.bottom() - visible.bottom(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};

    #[test]
    fn partial_scroll_clipping_preserves_the_control_position() {
        let bounds = Bounds::new(point(px(20.0), px(10.0)), size(px(100.0), px(30.0)));
        for (visible, expected) in [
            (bounds, point(px(0.0), px(0.0))),
            // Top clipping leaves the native bottom edge stationary.
            (
                Bounds::new(point(px(20.0), px(25.0)), size(px(100.0), px(15.0))),
                point(px(0.0), px(0.0)),
            ),
            // Bottom clipping shifts the host's local origin, not its content.
            (
                Bounds::new(point(px(20.0), px(10.0)), size(px(100.0), px(6.0))),
                point(px(0.0), px(24.0)),
            ),
            (
                Bounds::new(point(px(45.0), px(10.0)), size(px(75.0), px(30.0))),
                point(px(25.0), px(0.0)),
            ),
            (
                Bounds::new(point(px(20.0), px(10.0)), size(px(40.0), px(30.0))),
                point(px(0.0), px(0.0)),
            ),
        ] {
            assert_eq!(content_origin(bounds, visible), expected);
            let viewport_point = point(px(50.0), px(14.0));
            let local = point(
                viewport_point.x - bounds.left(),
                bounds.bottom() - viewport_point.y,
            );
            let translated = point(
                visible.left() + local.x - expected.x,
                visible.bottom() - local.y + expected.y,
            );
            assert_eq!(translated, viewport_point);
        }
    }
}
