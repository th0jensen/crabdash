//! Pixel panel geometry is independent of the integral terminal/PTY grid.
use super::sizing::Sizing;
pub(super) const CONTENT_SIDE: f32 = 10.0;
pub(super) const CONTENT_TOP: f32 = 16.0;
pub(super) const CONTENT_BOTTOM: f32 = 10.0;
pub(super) const PANEL_BORDER: f32 = 1.0;
pub(super) const DASHBOARD_RESERVE: f32 = 80.0;
const MIN_ROWS: u16 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Grid {
    pub columns: u16,
    pub rows: u16,
}
#[derive(Clone, Copy)]
pub(super) struct Geometry {
    width: f32,
    height: f32,
    header: f32,
    cell_width: f32,
    cell_height: f32,
    dashboard_reserve: f32,
}
fn nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}
fn cell(value: f32) -> f32 {
    nonnegative(value).max(1.0)
}
fn count(available: f32, cell: f32) -> u16 {
    (nonnegative(available) / cell)
        .floor()
        .clamp(1.0, f32::from(u16::MAX)) as u16
}
pub(super) fn height_for_rows(rows: u16, cell_height: f32, header: f32) -> f32 {
    nonnegative(header)
        + PANEL_BORDER
        + CONTENT_TOP
        + CONTENT_BOTTOM
        + f32::from(rows) * cell(cell_height)
}
#[cfg(test)]
pub(super) fn rows_for_height(height: f32, cell_height: f32, header: f32) -> u16 {
    count(
        height - height_for_rows(0, cell_height, header),
        cell(cell_height),
    )
}
/// Move by the actual mousedown displacement, preserving the grabbed offset.
pub(super) fn dragged_height(initial_height: f32, pointer_down: f32, pointer_now: f32) -> f32 {
    initial_height + pointer_down - pointer_now
}
impl Geometry {
    pub fn new(width: f32, height: f32, header: f32, cell_width: f32, cell_height: f32) -> Self {
        Self {
            width: nonnegative(width),
            height: nonnegative(height),
            header: nonnegative(header),
            cell_width: cell(cell_width),
            cell_height: cell(cell_height),
            dashboard_reserve: DASHBOARD_RESERVE,
        }
    }
    pub fn with_dashboard_reserve(mut self, reserve: f32) -> Self {
        self.dashboard_reserve = nonnegative(reserve);
        self
    }
    pub fn clamp_height(self, requested: f32) -> f32 {
        let reserve_max = (self.height - self.dashboard_reserve).max(0.0);
        // Tiny windows surrender only the reserve needed for one terminal row.
        let max =
            reserve_max.max(height_for_rows(1, self.cell_height, self.header).min(self.height));
        let min = height_for_rows(MIN_ROWS, self.cell_height, self.header).min(max);
        nonnegative(requested).clamp(min, max)
    }
    #[cfg(test)]
    pub fn grid(self, panel_height: f32) -> Grid {
        let columns = count(self.width - CONTENT_SIDE * 2.0, self.cell_width);
        let rows = rows_for_height(
            self.clamp_height(panel_height),
            self.cell_height,
            self.header,
        );
        Grid { columns, rows }
    }

    #[cfg(test)]
    pub(super) fn sizing(self, panel_height: f32, scale: f32) -> Sizing {
        self.pane_sizing(
            (self.clamp_height(panel_height) - PANEL_BORDER).max(0.0),
            scale,
        )
    }

    /// An assigned split pane already has bounded geometry; do not reserve
    /// dashboard space or clamp it as though it were the whole drawer.
    pub(super) fn pane_sizing(self, pane_height: f32, scale: f32) -> Sizing {
        let content_height =
            nonnegative(nonnegative(pane_height) - self.header - CONTENT_TOP - CONTENT_BOTTOM);
        Sizing::new(
            Grid {
                columns: count(self.width - CONTENT_SIDE * 2.0, self.cell_width),
                rows: count(content_height, self.cell_height),
            },
            (nonnegative(self.width - CONTENT_SIDE * 2.0), content_height),
            (self.cell_width, self.cell_height),
            scale,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::style;
    use crate::features::workspaces::model::{Axis, Drop, Layout, Node, Tab};

    #[test]
    fn maximum_drawer_retains_visible_bodies_in_stacked_and_mixed_dashboards() {
        fn check(node: &Node, width: f32, height: f32, scale: f32) {
            match node {
                Node::Pane { .. } => assert!(height > style::BAR * scale),
                Node::Split {
                    axis,
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    let extent = if *axis == Axis::Horizontal {
                        width
                    } else {
                        height
                    };
                    let ratio = crate::layout::geometry::usable_ratio(
                        *ratio,
                        extent,
                        crate::content::minimum_pane_extent(first, *axis, scale),
                        crate::content::minimum_pane_extent(second, *axis, scale),
                    );
                    let a = (extent - 1.0).max(0.0) * ratio;
                    let b = (extent - 1.0 - a).max(0.0);
                    if *axis == Axis::Horizontal {
                        check(first, a, height, scale);
                        check(second, b, height, scale);
                    } else {
                        check(first, width, a, scale);
                        check(second, width, b, scale);
                    }
                }
            }
        }
        for first_edge in [Drop::Bottom, Drop::Right] {
            let mut layout = Layout::default();
            assert!(layout.drop_tab(Tab::Disks, 1, first_edge));
            assert!(layout.drop_tab(Tab::Services, layout.focused, Drop::Bottom));
            assert!(layout.drop_tab(Tab::System, layout.focused, Drop::Bottom));
            assert!(layout.validate_members(&Tab::ALL).is_ok());
            for scale in [1.0, 20.0 / 14.0] {
                for titlebar in [0.0, style::TITLE_BAR * scale] {
                    let reserve = (DASHBOARD_RESERVE * scale).max(
                        titlebar + crate::content::minimum_visible_height(&layout.root, scale),
                    );
                    let geometry = Geometry::new(1100.0, 740.0, 64.0 * scale, 7.8, 20.0)
                        .with_dashboard_reserve(reserve);
                    let main_height = 740.0 - geometry.clamp_height(2000.0) - titlebar;
                    check(&layout.root, 1100.0, main_height, scale);
                }
            }
        }
    }

    #[test]
    fn tiny_fallback_surrenders_only_the_space_for_one_terminal_row() {
        let geometry = Geometry::new(800.0, 150.0, 64.0, 7.8, 20.0);
        assert_eq!(geometry.clamp_height(2000.0), 111.0);
        assert_eq!(150.0 - geometry.clamp_height(2000.0), 39.0);
    }
    #[test]
    fn maximum_drawer_leaves_font_scaled_dashboard_chrome_and_body() {
        let viewport = 740.0;
        for scale in [1.0, 20.0 / 14.0] {
            let geometry = Geometry::new(1100.0, viewport, 64.0 * scale, 7.8, 20.0)
                .with_dashboard_reserve(DASHBOARD_RESERVE * scale);
            let drawer = geometry.clamp_height(2000.0);
            let remaining = viewport - drawer;
            assert!((remaining - DASHBOARD_RESERVE * scale).abs() < 0.001);
            for titlebar in [0.0, 34.0 * scale] {
                let body = remaining - titlebar - 32.0 * scale;
                assert!(body > 0.0);
            }
        }
    }

    #[test]
    fn dashboard_reserve_is_explicit_sanitized_and_surrenders_in_tiny_windows() {
        for reserve in [-10.0, f32::NAN, f32::INFINITY] {
            let geometry =
                Geometry::new(800.0, 740.0, 64.0, 7.8, 20.0).with_dashboard_reserve(reserve);
            assert_eq!(geometry.clamp_height(2000.0), 740.0);
        }
        let geometry = Geometry::new(800.0, 100.0, 64.0, 7.8, 20.0)
            .with_dashboard_reserve(DASHBOARD_RESERVE * 20.0 / 14.0);
        assert_eq!(geometry.clamp_height(2000.0), 100.0);
    }
    #[test]
    fn pixel_resizing_remains_smooth_between_integral_grid_changes() {
        let geometry = Geometry::new(800.0, 1000.0, 32.0, 8.0, 20.0);
        let initial = height_for_rows(10, 20.0, 32.0);
        assert_eq!(geometry.clamp_height(initial + 7.5), initial + 7.5);
        let before = geometry.grid(initial);
        let after = geometry.grid(initial + 19.0);
        assert_eq!((before.columns, before.rows), (after.columns, after.rows));
        assert_eq!(geometry.grid(initial + 20.0).rows, 11);
        assert_eq!(geometry.grid(initial).columns, 97);
    }
    #[test]
    fn tiny_windows_still_have_valid_grids_and_bounded_panels() {
        for height in [0.0, 30.0, 79.0, 100.0] {
            let geometry = Geometry::new(25.0, height, 32.0, 8.0, 20.0);
            let panel = geometry.clamp_height(300.0);
            assert!(panel >= 0.0 && panel <= height);
            assert_eq!(geometry.grid(panel).columns, 1);
            assert!(geometry.grid(panel).rows >= 1);
        }
    }
    #[test]
    fn runtime_grid_can_exceed_initial_preference_limit() {
        let geometry = Geometry::new(1200.0, 2000.0, 32.0, 8.0, 20.0);
        assert_eq!(geometry.grid(height_for_rows(80, 20.0, 32.0)).rows, 80);
        assert_eq!(geometry.clamp_height(3000.0), 1920.0);
    }
    #[test]
    fn grab_position_does_not_jump_and_window_shrink_reclamps_pixels() {
        assert_eq!(dragged_height(400.0, 601.0, 601.0), 400.0);
        assert_eq!(dragged_height(400.0, 601.0, 581.5), 419.5);
        let smaller = Geometry::new(800.0, 400.0, 32.0, 8.0, 20.0);
        assert_eq!(smaller.clamp_height(419.5), 320.0);
    }

    #[test]
    fn combined_header_and_outer_border_are_counted_once() {
        for scale in [1.0, 20.0 / 14.0] {
            let header = style::BAR * scale;
            let initial = height_for_rows(16, 20.0, header);
            let pane_height = initial - PANEL_BORDER;
            let sizing =
                Geometry::new(1100.0, pane_height, header, 7.8, 20.0).pane_sizing(pane_height, 2.0);
            assert_eq!(sizing.pty.rows, 16);
            assert_eq!(sizing.pty.pixel_height, 640);
            assert_eq!(sizing.pty.pixel_width, 2160);
        }
    }
}
