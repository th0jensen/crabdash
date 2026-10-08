//! The renderer and every visible PTY use the same assigned pane rectangles.
use super::geometry::{CONTENT_BOTTOM, CONTENT_TOP, Geometry, PANEL_BORDER};
use crate::components::style;
use crate::layout::{Axis, Node, geometry::usable_ratio};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub scale: f32,
    cell_height: f32,
}

impl Metrics {
    pub fn new(scale: f32, cell_height: f32) -> Self {
        Self {
            scale: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
            cell_height: if cell_height.is_finite() && cell_height > 0.0 {
                cell_height
            } else {
                1.0
            },
        }
    }
}

pub(super) fn drawer_minimum(node: &Node<Uuid>, metrics: Metrics) -> f32 {
    minimum(node, Axis::Vertical, metrics) + PANEL_BORDER
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct HeaderRole {
    pub resize: bool,
    pub actions: bool,
}

/// Only leaves touching the drawer's upper edge resize it; its rightmost leaf owns actions.
pub(super) fn header_role(node: &Node<Uuid>, pane: u32) -> HeaderRole {
    fn upper_leaves(node: &Node<Uuid>, out: &mut Vec<u32>) {
        match node {
            Node::Pane { id, .. } => out.push(*id),
            Node::Split {
                axis,
                first,
                second,
                ..
            } => {
                upper_leaves(first, out);
                if *axis == Axis::Horizontal {
                    upper_leaves(second, out);
                }
            }
        }
    }
    let mut upper = Vec::new();
    upper_leaves(node, &mut upper);
    HeaderRole {
        resize: upper.contains(&pane),
        actions: upper.last() == Some(&pane),
    }
}

pub(super) fn clamp_height(
    geometry: Geometry,
    requested: f32,
    node: Option<&Node<Uuid>>,
    metrics: Metrics,
) -> f32 {
    let minimum = node.map_or(0.0, |node| drawer_minimum(node, metrics));
    geometry.clamp_height(requested.max(minimum))
}

pub(super) fn viewport(window: &gpui::Window) -> (f32, f32) {
    let inset = crate::desktop::appearance::frame_inset(window) * 2.0;
    (
        f32::from((window.viewport_size().width - inset).max(gpui::px(0.0))),
        f32::from((window.viewport_size().height - inset).max(gpui::px(0.0))),
    )
}

pub(super) fn minimum(node: &Node<Uuid>, axis: Axis, metrics: Metrics) -> f32 {
    match node {
        Node::Pane { .. } => {
            if axis == Axis::Horizontal {
                160.0 * metrics.scale
            } else {
                (100.0 * metrics.scale).max(
                    style::BAR * metrics.scale + CONTENT_TOP + CONTENT_BOTTOM + metrics.cell_height,
                )
            }
        }
        Node::Split {
            axis: split,
            first,
            second,
            ..
        } => {
            let first = minimum(first, axis, metrics);
            let second = minimum(second, axis, metrics);
            if *split == axis {
                first + second + 1.0
            } else {
                first.max(second)
            }
        }
    }
}

pub(super) fn extents(
    axis: Axis,
    ratio: f32,
    first: &Node<Uuid>,
    second: &Node<Uuid>,
    width: f32,
    height: f32,
    metrics: Metrics,
) -> (f32, f32) {
    let extent = if axis == Axis::Horizontal {
        width
    } else {
        height
    };
    let ratio = usable_ratio(
        ratio,
        extent,
        minimum(first, axis, metrics),
        minimum(second, axis, metrics),
    );
    let first = (extent - 1.0).max(0.0) * ratio;
    (first, (extent - 1.0 - first).max(0.0))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Rect {
    pub pane: u32,
    pub session: Uuid,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub(super) fn rectangles(
    node: &Node<Uuid>,
    width: f32,
    height: f32,
    metrics: Metrics,
) -> Vec<Rect> {
    fn visit(node: &Node<Uuid>, rect: Rect, metrics: Metrics, out: &mut Vec<Rect>) {
        match node {
            Node::Pane { id, active, .. } => out.push(Rect {
                pane: *id,
                session: *active,
                ..rect
            }),
            Node::Split {
                axis,
                ratio,
                first,
                second,
                ..
            } => {
                let (a, b) = extents(
                    *axis,
                    *ratio,
                    first,
                    second,
                    rect.width,
                    rect.height,
                    metrics,
                );
                if *axis == Axis::Horizontal {
                    visit(first, Rect { width: a, ..rect }, metrics, out);
                    visit(
                        second,
                        Rect {
                            x: rect.x + a + 1.0,
                            width: b,
                            ..rect
                        },
                        metrics,
                        out,
                    );
                } else {
                    visit(first, Rect { height: a, ..rect }, metrics, out);
                    visit(
                        second,
                        Rect {
                            y: rect.y + a + 1.0,
                            height: b,
                            ..rect
                        },
                        metrics,
                        out,
                    );
                }
            }
        }
    }
    let mut out = Vec::new();
    visit(
        node,
        Rect {
            pane: 0,
            session: Uuid::nil(),
            x: 0.0,
            y: 0.0,
            width: width.max(0.0),
            height: height.max(0.0),
        },
        metrics,
        &mut out,
    );
    out
}

pub(super) fn neighbor(rects: &[Rect], focused: u32, direction: Direction) -> Option<Uuid> {
    let rect = rects.iter().find(|rect| rect.pane == focused)?;
    // Cross the one-pixel divider at the edge midpoint, into the adjacent pane.
    let (x, y) = match direction {
        Direction::Left => (rect.x - 1.5, rect.y + rect.height / 2.0),
        Direction::Right => (rect.x + rect.width + 1.5, rect.y + rect.height / 2.0),
        Direction::Up => (rect.x + rect.width / 2.0, rect.y - 1.5),
        Direction::Down => (rect.x + rect.width / 2.0, rect.y + rect.height + 1.5),
    };
    if let Some(other) = rects.iter().find(|other| {
        other.pane != focused
            && x >= other.x
            && x < other.x + other.width
            && y >= other.y
            && y < other.y + other.height
    }) {
        return Some(other.session);
    }
    // At a T junction the midpoint can land inside the perpendicular divider.
    // Choose the nearest overlapping neighbor, with traversal order breaking ties.
    let distance = |other: &Rect| -> Option<f32> {
        let (edge_gap, start, end, center, source_start, source_end) = match direction {
            Direction::Left => (
                rect.x - (other.x + other.width),
                other.y,
                other.y + other.height,
                y,
                rect.y,
                rect.y + rect.height,
            ),
            Direction::Right => (
                other.x - (rect.x + rect.width),
                other.y,
                other.y + other.height,
                y,
                rect.y,
                rect.y + rect.height,
            ),
            Direction::Up => (
                rect.y - (other.y + other.height),
                other.x,
                other.x + other.width,
                x,
                rect.x,
                rect.x + rect.width,
            ),
            Direction::Down => (
                other.y - (rect.y + rect.height),
                other.x,
                other.x + other.width,
                x,
                rect.x,
                rect.x + rect.width,
            ),
        };
        (other.pane != focused
            && (edge_gap - 1.0).abs() < 0.01
            && start < source_end
            && end > source_start)
            .then(|| (center - center.clamp(start, end)).abs())
    };
    rects
        .iter()
        .filter_map(|other| distance(other).map(|distance| (other.session, distance)))
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(session, _)| session)
}

pub(super) fn visible(
    node: &Node<Uuid>,
    width: f32,
    height: f32,
    metrics: Metrics,
) -> Vec<(Uuid, f32, f32)> {
    rectangles(node, width, height, metrics)
        .into_iter()
        .map(|rect| (rect.session, rect.width, rect.height))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Drop, Layout};

    #[test]
    fn only_upper_edge_headers_resize_and_upper_right_header_owns_actions() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let d = Uuid::new_v4();
        let mut layout = Layout::single(vec![a, b, c, d], a);
        assert_eq!(
            header_role(&layout.root, 1),
            HeaderRole {
                resize: true,
                actions: true
            }
        );
        assert_eq!(header_role(&layout.root, 99), HeaderRole::default());
        assert!(layout.drop_tab(b, 1, Drop::Right));
        let right = layout.focused;
        assert_eq!(
            header_role(&layout.root, 1),
            HeaderRole {
                resize: true,
                actions: false
            }
        );
        assert_eq!(
            header_role(&layout.root, right),
            HeaderRole {
                resize: true,
                actions: true
            }
        );
        assert!(layout.drop_tab(c, 1, Drop::Bottom));
        let lower_left = layout.focused;
        assert_eq!(header_role(&layout.root, lower_left), HeaderRole::default());
        assert!(layout.drop_tab(d, right, Drop::Bottom));
        let lower_right = layout.focused;
        assert_eq!(
            header_role(&layout.root, lower_right),
            HeaderRole::default()
        );
        assert_eq!(
            header_role(&layout.root, right),
            HeaderRole {
                resize: true,
                actions: true
            }
        );
    }

    #[test]
    fn a_single_pane_drawer_reserves_one_header_and_one_outer_border() {
        let tab = Uuid::new_v4();
        let layout = Layout::single(vec![tab], tab);
        assert_eq!(
            drawer_minimum(&layout.root, Metrics::new(1.0, 100.0)),
            159.0
        );
        assert!(
            (drawer_minimum(&layout.root, Metrics::new(20.0 / 14.0, 100.0)) - 172.7143).abs()
                < 0.001
        );
    }

    #[test]
    fn directional_focus_uses_assigned_rectangles_without_wrapping() {
        let left = Uuid::new_v4();
        let above = Uuid::new_v4();
        let below = Uuid::new_v4();
        let mut layout = Layout::single(vec![left, above, below], left);
        assert!(layout.drop_tab(above, 1, Drop::Right));
        let right_pane = layout.focused;
        assert!(layout.drop_tab(below, right_pane, Drop::Bottom));
        let bottom_pane = layout.focused;
        assert!(layout.set_ratio(3, 0.3));
        assert!(layout.set_ratio(5, 0.7));
        let metrics = Metrics::new(1.0, 20.0);
        let rects = rectangles(&layout.root, 1001.0, 401.0, metrics);
        assert_eq!(neighbor(&rects, 1, Direction::Right), Some(above));
        assert_eq!(neighbor(&rects, right_pane, Direction::Down), Some(below));
        assert_eq!(neighbor(&rects, bottom_pane, Direction::Up), Some(above));
        assert_eq!(neighbor(&rects, bottom_pane, Direction::Left), Some(left));
        assert_eq!(neighbor(&rects, 1, Direction::Left), None);
        assert_eq!(neighbor(&rects, bottom_pane, Direction::Down), None);
        assert_eq!(neighbor(&rects, bottom_pane, Direction::Right), None);
        assert_eq!(neighbor(&rects, 99, Direction::Right), None);
        // Minimum widths override the raw ratio, just as they do when painting.
        assert!(layout.set_ratio(3, 0.15));
        let narrow = rectangles(&layout.root, 401.0, 401.0, metrics);
        assert_eq!(narrow[0].width, 160.0);
        assert_eq!(neighbor(&narrow, 1, Direction::Right), Some(above));
    }

    #[test]
    fn midpoint_on_a_t_junction_divider_selects_the_nearest_neighbor() {
        let left = Uuid::new_v4();
        let above = Uuid::new_v4();
        let below = Uuid::new_v4();
        let mut layout = Layout::single(vec![left, above, below], left);
        assert!(layout.drop_tab(above, 1, Drop::Right));
        assert!(layout.drop_tab(below, layout.focused, Drop::Bottom));
        let rects = rectangles(&layout.root, 801.0, 401.0, Metrics::new(1.0, 20.0));
        assert_eq!(neighbor(&rects, 1, Direction::Right), Some(above));
        assert_eq!(rects[1].height, 200.0);
        assert_eq!(rects[2].y, 201.0);
    }

    #[test]
    fn visible_sessions_receive_exact_horizontal_and_vertical_allocations() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let third = Uuid::new_v4();
        let mut layout = Layout::single(vec![first, second, third], first);
        assert!(layout.drop_tab(second, 1, Drop::Right));
        assert!(layout.drop_tab(third, 1, Drop::Bottom));
        let metrics = Metrics::new(1.0, 20.0);
        let panes = visible(&layout.root, 801.0, 401.0, metrics);
        assert_eq!(panes.len(), 3);
        assert!(panes.contains(&(second, 400.0, 401.0)));
        assert!(panes.contains(&(first, 400.0, 200.0)));
        assert!(panes.contains(&(third, 400.0, 200.0)));
        assert!(layout.remove(third));
        assert!(layout.remove(second));
        assert_eq!(
            visible(&layout.root, 801.0, 401.0, metrics),
            vec![(first, 801.0, 401.0)]
        );
    }

    #[test]
    fn shrinking_a_split_drawer_retains_a_drawable_row_in_each_pane() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let third = Uuid::new_v4();
        let mut layout = Layout::single(vec![first, second, third], first);
        assert!(layout.drop_tab(second, 1, Drop::Right));
        assert!(layout.drop_tab(third, 1, Drop::Bottom));
        for (scale, cell_height) in [(1.0, 20.0), (20.0 / 14.0, 20.0), (20.0 / 14.0, 80.0)] {
            let metrics = Metrics::new(scale, cell_height);
            let geometry = Geometry::new(1100.0, 1200.0, style::BAR * scale, 7.8, cell_height);
            let height = clamp_height(geometry, 0.0, Some(&layout.root), metrics);
            assert!(height >= drawer_minimum(&layout.root, metrics));
            assert_eq!(
                clamp_height(geometry, height, Some(&layout.root), metrics),
                height
            );
            let body = height - PANEL_BORDER;
            let assigned = visible(&layout.root, 1100.0, body, metrics);
            assert_eq!(assigned.len(), 3);
            for (_, width, pane_height) in assigned {
                let sizing =
                    Geometry::new(width, pane_height, style::BAR * scale, 7.8, cell_height)
                        .pane_sizing(pane_height, 2.0);
                assert!(sizing.pty.rows >= 1);
                assert!(u32::from(sizing.pty.pixel_height) >= sizing.emulator.cell_height);
            }
        }
    }

    #[test]
    fn impossible_split_minima_remain_bounded_by_the_available_window() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut layout = Layout::single(vec![first, second], first);
        assert!(layout.drop_tab(second, 1, Drop::Bottom));
        let metrics = Metrics::new(1.25, 80.0);
        let geometry = Geometry::new(800.0, 150.0, 40.0, 7.8, 80.0);
        let height = clamp_height(geometry, 0.0, Some(&layout.root), metrics);
        assert_eq!(height, 147.0);
        assert_eq!(150.0 - height, 3.0);
        assert!(height < drawer_minimum(&layout.root, metrics));
        let body = height - PANEL_BORDER;
        let assigned = visible(&layout.root, 800.0, body, metrics);
        assert_eq!(assigned.len(), 2);
        assert_eq!(assigned[0].2 + assigned[1].2 + 1.0, body);
        for (_, width, pane_height) in assigned {
            assert!(pane_height >= 0.0 && pane_height <= body);
            let sizing =
                Geometry::new(width, pane_height, 40.0, 7.8, 80.0).pane_sizing(pane_height, 2.0);
            assert!(f32::from(sizing.pty.pixel_height) <= pane_height * 2.0);
            assert_eq!(sizing.pty.rows, 1);
        }
    }
}
