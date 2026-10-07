//! GPUI uses a top-left origin inside its renderer view, not its containing window.
use crate::{Bounds, Pixels, Point, point, px};
use cocoa::foundation::{NSPoint, NSRect, NSSize};

pub(super) fn local_point(position: NSPoint, bounds: NSRect, flipped: bool) -> Point<Pixels> {
    point(
        px((position.x - bounds.origin.x) as f32),
        px(if flipped {
            position.y - bounds.origin.y
        } else {
            bounds.origin.y + bounds.size.height - position.y
        } as f32),
    )
}

pub(super) fn local_rect(rect: Bounds<Pixels>, bounds: NSRect, flipped: bool) -> NSRect {
    NSRect::new(
        NSPoint::new(
            bounds.origin.x + f64::from(f32::from(rect.origin.x)),
            if flipped {
                bounds.origin.y + f64::from(f32::from(rect.origin.y))
            } else {
                bounds.origin.y + bounds.size.height
                    - f64::from(f32::from(rect.origin.y + rect.size.height))
            },
        ),
        NSSize::new(
            f64::from(f32::from(rect.size.width)),
            f64::from(f32::from(rect.size.height)),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::size;
    use std::prelude::v1::test;

    #[test]
    fn renderer_origin_is_independent_of_native_bounds_origin() {
        let bounds = NSRect::new(NSPoint::new(7.0, 11.0), NSSize::new(600.0, 400.0));
        assert_eq!(
            local_point(NSPoint::new(37.0, 361.0), bounds, false),
            point(px(30.0), px(50.0))
        );
        assert_eq!(
            local_point(NSPoint::new(37.0, 61.0), bounds, true),
            point(px(30.0), px(50.0))
        );
        // Outside points remain outside for drag exits and hover hit testing.
        assert_eq!(
            local_point(NSPoint::new(2.0, 421.0), bounds, false),
            point(px(-5.0), px(-10.0))
        );
    }

    #[test]
    fn ime_rect_round_trips_in_flipped_and_unflipped_renderer_views() {
        let bounds = NSRect::new(NSPoint::new(7.0, 11.0), NSSize::new(600.0, 400.0));
        let caret = Bounds::new(point(px(30.0), px(50.0)), size(px(2.0), px(18.0)));
        for flipped in [false, true] {
            let native = local_rect(caret, bounds, flipped);
            let top = NSPoint::new(
                native.origin.x,
                native.origin.y + if flipped { 0.0 } else { native.size.height },
            );
            assert_eq!(local_point(top, bounds, flipped), caret.origin);
            assert_eq!(native.size.width, 2.0);
            assert_eq!(native.size.height, 18.0);
        }
    }
}
