//! The emulator uses integral cells; the PTY reports the full drawable viewport.
use super::geometry::Grid;
use machines::terminal::TerminalSize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EmulatorSize {
    pub columns: u16,
    pub rows: u16,
    pub cell_width: u32,
    pub cell_height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Sizing {
    pub emulator: EmulatorSize,
    pub pty: TerminalSize,
}

fn viewport_pixels(logical: f32, scale: f64) -> u16 {
    let logical = if logical.is_finite() {
        f64::from(logical.max(0.0))
    } else {
        0.0
    };
    (logical * scale).floor().clamp(0.0, f64::from(u16::MAX)) as u16
}

fn cell_pixels(logical: f32, scale: f64) -> u32 {
    let logical = if logical.is_finite() && logical > 0.0 {
        f64::from(logical)
    } else {
        1.0
    };
    (logical * scale).ceil().clamp(1.0, f64::from(u32::MAX)) as u32
}

impl Sizing {
    pub(super) fn new(grid: Grid, viewport: (f32, f32), cell: (f32, f32), scale: f32) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            f64::from(scale)
        } else {
            1.0
        };
        Self {
            emulator: EmulatorSize {
                columns: grid.columns,
                rows: grid.rows,
                cell_width: cell_pixels(cell.0, scale),
                cell_height: cell_pixels(cell.1, scale),
            },
            pty: TerminalSize {
                columns: grid.columns,
                rows: grid.rows,
                pixel_width: viewport_pixels(viewport.0, scale),
                pixel_height: viewport_pixels(viewport.1, scale),
            },
        }
    }

    /// Pixel-only viewport changes must not reset emulator state.
    pub(super) fn changes(self, emulator: EmulatorSize, pty: TerminalSize) -> (bool, bool) {
        (self.emulator != emulator, self.pty != pty)
    }
}

#[cfg(test)]
mod tests {
    use super::cell_pixels;
    use crate::features::terminal::geometry::{Geometry, height_for_rows};

    #[test]
    fn fractional_cells_report_drawable_pixels_and_keep_leftover_height() {
        let geometry = Geometry::new(1100.0, 1000.0, 32.0, 7.8, 20.0);
        let sizing = geometry.sizing(height_for_rows(16, 20.0, 32.0) + 7.75, 1.0);
        assert_eq!(sizing.pty.columns, 138);
        assert_eq!(sizing.pty.rows, 16);
        assert_eq!(sizing.pty.pixel_width, 1080);
        assert_eq!(sizing.pty.pixel_height, 327);
        assert_eq!(sizing.emulator.cell_width, 8);
        assert_eq!(sizing.emulator.cell_height, 20);
    }

    #[test]
    fn scale_changes_physical_pixels_without_changing_the_logical_grid() {
        let geometry = Geometry::new(1100.0, 1000.0, 32.0, 7.8, 20.0);
        let height = height_for_rows(16, 20.0, 32.0);
        let logical = geometry.sizing(height, 1.0);
        let doubled = geometry.sizing(height, 2.0);
        assert_eq!(doubled.pty.columns, logical.pty.columns);
        assert_eq!(doubled.pty.rows, logical.pty.rows);
        assert_eq!(doubled.pty.pixel_width, logical.pty.pixel_width * 2);
        assert_eq!(doubled.pty.pixel_height, logical.pty.pixel_height * 2);
        assert_eq!(doubled.emulator.cell_width, 16);
        assert_eq!(doubled.emulator.cell_height, 40);
        assert_eq!(doubled.changes(logical.emulator, logical.pty), (true, true));
    }

    #[test]
    fn emulator_metrics_and_pty_viewport_changes_are_independent() {
        let geometry = Geometry::new(1100.0, 1000.0, 32.0, 7.8, 20.0);
        let height = height_for_rows(16, 20.0, 32.0);
        let previous = geometry.sizing(height, 1.0);
        let taller = geometry.sizing(height + 7.0, 1.0);
        assert_eq!(
            taller.changes(previous.emulator, previous.pty),
            (false, true)
        );
        assert_eq!(
            previous.changes(previous.emulator, previous.pty),
            (false, false)
        );

        // A font change can cross an integer physical-cell boundary while
        // retaining the same rows, columns, and drawable viewport.
        let font_changed =
            Geometry::new(1100.0, 1000.0, 32.0, 7.81, 20.01).sizing(height + 7.0, 1.0);
        let previous = Geometry::new(1100.0, 1000.0, 32.0, 7.8, 20.0).sizing(height + 7.0, 1.0);
        assert_eq!(font_changed.pty.columns, previous.pty.columns);
        assert_eq!(font_changed.pty.rows, previous.pty.rows);
        assert_eq!(
            font_changed.changes(previous.emulator, previous.pty),
            (true, false)
        );
    }

    #[test]
    fn tiny_invalid_and_large_dimensions_are_bounded() {
        let tiny = Geometry::new(0.0, 0.0, 32.0, 7.8, 20.0).sizing(0.0, 2.0);
        assert_eq!((tiny.pty.columns, tiny.pty.rows), (1, 1));
        assert_eq!((tiny.pty.pixel_width, tiny.pty.pixel_height), (0, 0));
        let invalid = Geometry::new(f32::NAN, f32::INFINITY, f32::NAN, f32::NAN, 0.0)
            .sizing(f32::NAN, f32::INFINITY);
        assert_eq!((invalid.pty.columns, invalid.pty.rows), (1, 1));
        assert_eq!((invalid.pty.pixel_width, invalid.pty.pixel_height), (0, 0));
        assert_eq!(
            (invalid.emulator.cell_width, invalid.emulator.cell_height),
            (1, 1)
        );
        let huge = Geometry::new(f32::MAX, f32::MAX, 0.0, 1.0, 1.0).sizing(f32::MAX, f32::MAX);
        assert_eq!(
            (huge.pty.pixel_width, huge.pty.pixel_height),
            (u16::MAX, u16::MAX)
        );
        assert_eq!(huge.emulator.cell_width, u32::MAX);
        assert_eq!(cell_pixels(0.0, 0.01), 1);
    }
}
