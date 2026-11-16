//! Pointer input from touch clients such as Rex for iOS, following ramp: a tap
//! is a press and release on the same cell with no drag between, so a swipe or
//! a selection drag never activates a control. The wheel scrolls content.
use crate::section::Section;
use ratatui::{
    crossterm::event::KeyCode,
    layout::{Position, Rect},
};

/// Below this width the dashboard uses its phone layout, as ramp does.
pub const DESKTOP_WIDTH: u16 = 80;

#[derive(Default)]
pub struct Pointer {
    down: Option<(Position, bool)>,
}

impl Pointer {
    pub fn press(&mut self, at: Position) {
        self.down = Some((at, false));
    }

    pub fn drag(&mut self) {
        if let Some((_, moved)) = &mut self.down {
            *moved = true;
        }
    }

    /// Whether this release completes a tap.
    pub fn release(&mut self, at: Position) -> bool {
        matches!(self.down.take(), Some((down, false)) if down == at)
    }
}

/// What the last frame drew where, so a tap can find its target.
#[derive(Default)]
pub struct Hits {
    pub tabs: Vec<(Rect, Section)>,
    /// Buttons, pills and key hints, each standing for the key it presses.
    pub keys: Vec<(Rect, KeyCode)>,
    /// The table body and the index of its first visible row.
    pub rows: Option<(Rect, usize)>,
    /// An overlay that covers the rows beneath it.
    pub overlay: Option<Rect>,
}

impl Hits {
    pub fn key(&self, at: Position) -> Option<KeyCode> {
        self.keys
            .iter()
            .find(|(area, _)| area.contains(at))
            .map(|(_, key)| *key)
    }

    pub fn tab(&self, at: Position) -> Option<Section> {
        self.tabs
            .iter()
            .find(|(area, _)| area.contains(at))
            .map(|(_, section)| *section)
    }

    /// The table row under `at`, unless an overlay covers it.
    pub fn row(&self, at: Position) -> Option<usize> {
        if self.overlay.is_some_and(|overlay| overlay.contains(at)) {
            return None;
        }
        let (body, offset) = self.rows?;
        body.contains(at)
            .then(|| offset + usize::from(at.y - body.y))
    }

    /// Rows the table body shows at once.
    pub fn visible_rows(&self) -> usize {
        self.rows
            .map_or(1, |(body, _)| usize::from(body.height).max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_still_press_and_release_on_one_cell_is_a_tap() {
        let mut pointer = Pointer::default();
        pointer.press(Position::new(3, 4));
        assert!(pointer.release(Position::new(3, 4)));
        // A release with no press, as after a tap already completed.
        assert!(!pointer.release(Position::new(3, 4)));
        pointer.press(Position::new(3, 4));
        assert!(!pointer.release(Position::new(4, 4)));
        pointer.press(Position::new(3, 4));
        pointer.drag();
        assert!(!pointer.release(Position::new(3, 4)));
    }

    #[test]
    fn an_overlay_hides_the_rows_beneath_it() {
        let hits = Hits {
            rows: Some((Rect::new(0, 5, 20, 10), 7)),
            overlay: Some(Rect::new(0, 12, 20, 3)),
            ..Hits::default()
        };
        assert_eq!(hits.row(Position::new(1, 5)), Some(7));
        assert_eq!(hits.row(Position::new(1, 11)), Some(13));
        assert_eq!(hits.row(Position::new(1, 12)), None);
        assert_eq!(hits.row(Position::new(1, 4)), None);
    }
}
