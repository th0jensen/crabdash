//! Preserve tracking transitions until publishing them outside native layout.
use std::collections::VecDeque;

pub(super) struct Hover<T> {
    hovered: bool,
    pending: bool,
    events: VecDeque<(Option<bool>, Option<T>)>,
}

impl<T> Default for Hover<T> {
    fn default() -> Self {
        Self {
            hovered: false,
            pending: false,
            events: VecDeque::new(),
        }
    }
}

impl<T> Hover<T> {
    pub(super) fn hovered(&self) -> bool {
        self.hovered
    }

    /// Return whether the caller should schedule a deferred drain.
    pub(super) fn record(&mut self, hovered: bool, exit: Option<T>) -> bool {
        let changed = self.hovered != hovered;
        self.hovered = hovered;
        if !changed && exit.is_none() {
            return false;
        }
        self.events.push_back((changed.then_some(hovered), exit));
        let wake = !self.pending;
        self.pending = true;
        wake
    }

    pub(super) fn take(&mut self) -> Option<(Option<bool>, Option<T>)> {
        let event = self.events.pop_front();
        if event.is_none() {
            self.pending = false;
        }
        event
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::Hover;

    #[test]
    fn final_hover_does_not_coalesce_an_exit_before_synchronous_motion() {
        let mut hover = Hover::default();
        assert!(hover.record(true, None));
        assert!(!hover.record(false, Some("exit")));
        assert!(!hover.record(true, None));
        assert!(hover.hovered());
        assert_eq!(hover.take(), Some((Some(true), None)));
        assert_eq!(hover.take(), Some((Some(false), Some("exit"))));
        assert_eq!(hover.take(), Some((Some(true), None)));
        assert_eq!(hover.take(), None);
        assert!(hover.record(false, None));
    }

    #[test]
    fn duplicate_exit_input_is_retained_but_removal_discards_queued_callbacks() {
        let mut hover = Hover::default();
        assert!(hover.record(false, Some("exit")));
        assert_eq!(hover.take(), Some((None, Some("exit"))));
        assert!(!hover.record(true, None));
        hover.clear();
        assert_eq!(hover.take(), None);
        assert!(!hover.hovered());
        assert!(hover.record(true, None));
    }
}
