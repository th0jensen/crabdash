//! Pointer input from touch clients such as Rex for iOS, following ramp: a tap
//! is a press and release on the same cell with no drag between, so a swipe or
//! a selection drag never activates a control. The wheel scrolls content.
use crate::section::Section;
use ratatui::{
    crossterm::event::KeyCode,
    layout::{Position, Rect},
};

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
    /// Each visible table row's first line and index. An expanded row's
    /// details take the lines below, which are not part of its target.
    pub rows: Vec<(Rect, usize)>,
    /// The table body, below its header.
    pub body: Option<Rect>,
    /// An overlay that covers the rows beneath it.
    pub overlay: Option<Rect>,
    /// The search field.
    pub search: Option<Rect>,
    /// Filter or sort chips, by index.
    pub chips: Vec<(Rect, usize)>,
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

    pub fn chip(&self, at: Position) -> Option<usize> {
        self.chips
            .iter()
            .find(|(area, _)| area.contains(at))
            .map(|(_, chip)| *chip)
    }

    /// The table row under `at`, unless an overlay covers it.
    pub fn row(&self, at: Position) -> Option<usize> {
        if self.overlay.is_some_and(|overlay| overlay.contains(at)) {
            return None;
        }
        self.rows
            .iter()
            .find(|(area, _)| area.contains(at))
            .map(|(_, index)| *index)
    }

    /// Rows the table body shows at once.
    pub fn visible_rows(&self) -> usize {
        self.body.map_or(1, |body| usize::from(body.height).max(1))
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
            rows: (0..10)
                .map(|i| (Rect::new(0, 5 + i, 20, 1), 7 + usize::from(i)))
                .collect(),
            overlay: Some(Rect::new(0, 12, 20, 3)),
            ..Hits::default()
        };
        assert_eq!(hits.row(Position::new(1, 5)), Some(7));
        assert_eq!(hits.row(Position::new(1, 11)), Some(13));
        assert_eq!(hits.row(Position::new(1, 12)), None);
        assert_eq!(hits.row(Position::new(1, 4)), None);
    }
}
