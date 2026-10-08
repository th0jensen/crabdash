//! Splitter geometry shared by the first threshold move and later drag moves.
use crate::{app::Crabdash, features::workspaces::model::Axis};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};
use uuid::Uuid;

#[derive(Clone)]
struct SplitResize {
    id: u32,
    owner: EntityId,
    workspace: Uuid,
    revision: Rc<Cell<u64>>,
    grab: Rc<Cell<Option<Pixels>>>,
}

impl Render for SplitResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

use crate::layout::geometry::drag_ratio;
pub(super) use crate::layout::geometry::usable_ratio;

#[derive(Clone, Copy)]
pub(super) struct Geometry {
    pub axis: Axis,
    pub first_min: f32,
    pub second_min: f32,
}

impl Geometry {
    fn coordinate(self, position: Point<Pixels>) -> Pixels {
        if self.axis == Axis::Horizontal {
            position.x
        } else {
            position.y
        }
    }

    fn extent(self, bounds: Bounds<Pixels>) -> Pixels {
        if self.axis == Axis::Horizontal {
            bounds.size.width
        } else {
            bounds.size.height
        }
    }

    fn apply(
        self,
        app: &mut Crabdash,
        drag: &SplitResize,
        bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        cx: &mut Context<Crabdash>,
    ) {
        if drag.owner != cx.entity_id()
            || drag.workspace != app.workspaces.store.active
            || drag.revision.get() != app.workspaces.revision()
            || app.workspaces.resizing_split != Some(drag.id)
        {
            return;
        }
        let Some(grab) = drag.grab.get() else {
            return;
        };
        if let Some(ratio) = drag_ratio(
            f32::from(self.coordinate(position)),
            f32::from(self.coordinate(bounds.origin)),
            f32::from(self.extent(bounds)),
            f32::from(grab),
            self.first_min,
            self.second_min,
        ) {
            app.workspaces.drag_target = None;
            if app.workspaces.layout_mut().set_ratio(drag.id, ratio) {
                cx.notify();
            }
        }
    }
}

pub(super) fn divider(
    app: &Crabdash,
    id: u32,
    first_extent: Pixels,
    geometry: Geometry,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let down_bounds = bounds.clone();
    let owner = cx.entity().downgrade();
    crate::components::split_handle::render(
        SharedString::from(format!("workspace-divider-{id}")),
        geometry.axis,
        first_extent,
    )
    .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |app, event: &MouseDownEvent, _, _| {
            app.workspaces.split_resize_grab = down_bounds.get().map(|bounds| {
                let center = geometry.coordinate(bounds.origin) + first_extent + px(0.5);
                (id, geometry.coordinate(event.position) - center)
            });
        }),
    )
    .on_drag(
        SplitResize {
            id,
            owner: cx.entity_id(),
            workspace: app.workspaces.store.active,
            revision: Rc::new(Cell::new(app.workspaces.revision())),
            grab: Rc::new(Cell::new(None)),
        },
        move |drag, _, window, cx| {
            owner
                .update(cx, |app, cx| {
                    let grab = app.workspaces.split_resize_grab.take();
                    if drag.owner != cx.entity_id()
                        || drag.workspace != app.workspaces.store.active
                        || grab.is_none_or(|(anchor, _)| anchor != id)
                    {
                        return;
                    }
                    drag.grab.set(grab.map(|(_, grab)| grab));
                    // Focus may have persisted between mouse-down and drag
                    // construction. Stamp the current shared-store revision.
                    drag.revision.set(app.workspaces.revision());
                    app.workspaces.drag_target = None;
                    app.workspaces.resizing_split = Some(id);
                    // GPUI starts active_drag in Bubble, after on_drag_move's
                    // Capture phase. Apply that first movement here too.
                    if let Some(bounds) = bounds.get() {
                        geometry.apply(app, drag, bounds, window.mouse_position(), cx);
                    }
                    cx.notify();
                })
                .ok();
            cx.new(|_| drag.clone())
        },
    )
}

pub(super) fn moves(
    element: Stateful<Div>,
    id: u32,
    geometry: Geometry,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    element.on_drag_move(
        cx.listener(move |app, event: &DragMoveEvent<SplitResize>, _, cx| {
            let drag = event.drag(cx).clone();
            if drag.id == id {
                geometry.apply(app, &drag, event.bounds, event.event.position, cx);
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::test;

    #[test]
    fn first_movement_and_later_movements_use_the_same_anchored_geometry() {
        let origin = 252.0;
        let extent = 961.0;
        let center = origin + 480.0 + 0.5;
        let down = center + 2.5;
        let grab = down - center;
        assert_eq!(
            drag_ratio(down, origin, extent, grab, 224.0, 224.0),
            Some(0.5)
        );
        assert_eq!(
            drag_ratio(down + 96.0, origin, extent, grab, 224.0, 224.0),
            Some(0.6)
        );
        assert_eq!(
            drag_ratio(down - 96.0, origin, extent, grab, 224.0, 224.0),
            Some(0.4)
        );
    }

    #[test]
    fn clamps_to_subtree_minima_and_handles_tiny_or_invalid_extents() {
        assert_eq!(drag_ratio(0.0, 0.0, 1001.0, 0.0, 400.0, 200.0), Some(0.4));
        assert_eq!(
            drag_ratio(2000.0, 0.0, 1001.0, 0.0, 400.0, 200.0),
            Some(0.8)
        );
        assert_eq!(
            drag_ratio(20.0, 0.0, 100.0, 0.0, 400.0, 200.0),
            Some(2.0 / 3.0)
        );
        assert_eq!(drag_ratio(20.0, 0.0, 1.0, 0.0, 400.0, 200.0), None);
        assert_eq!(drag_ratio(f32::NAN, 0.0, 1001.0, 0.0, 400.0, 200.0), None);
    }
}
