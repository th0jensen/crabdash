//! Visible dashboard chrome and the existing pane resize minima.
use crate::components::style;
use crate::features::workspaces::model::{Axis, Node};

pub(super) fn minimum_extent(node: &Node, axis: Axis, scale: f32) -> f32 {
    match node {
        Node::Pane { .. } => match axis {
            Axis::Horizontal => 200.0 * scale + 24.0,
            Axis::Vertical => 200.0 * scale + 12.0,
        },
        Node::Split {
            axis: split_axis,
            first,
            second,
            ..
        } => {
            let first = minimum_extent(first, axis, scale);
            let second = minimum_extent(second, axis, scale);
            if *split_axis == axis {
                first + second + 1.0
            } else {
                first.max(second)
            }
        }
    }
}

pub(crate) fn minimum_visible_height(node: &Node, scale: f32) -> f32 {
    match node {
        Node::Pane { .. } => (style::BAR + style::TEXT) * scale,
        Node::Split {
            axis,
            first,
            second,
            ..
        } => {
            let first = minimum_visible_height(first, scale);
            let second = minimum_visible_height(second, scale);
            if *axis == Axis::Vertical {
                first + second + 1.0
            } else {
                first.max(second)
            }
        }
    }
}
