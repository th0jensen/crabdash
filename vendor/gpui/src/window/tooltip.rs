//! Real pointer presence and lifetime tokens for window-local tooltips.
pub(super) struct Presence {
    hovered: bool,
    active: bool,
    epoch: u64,
}

impl Presence {
    pub(super) fn new(hovered: bool, active: bool) -> Self {
        Self {
            hovered,
            active,
            epoch: 0,
        }
    }

    pub(super) fn set_hovered(&mut self, hovered: bool) -> bool {
        let exited = self.hovered && !hovered;
        self.hovered = hovered;
        self.invalidate(exited)
    }

    pub(super) fn set_active(&mut self, active: bool) -> bool {
        let lost_focus = self.active && !active;
        self.active = active;
        self.invalidate(lost_focus)
    }

    fn invalidate(&mut self, invalidate: bool) -> bool {
        if invalidate {
            self.epoch = self.epoch.wrapping_add(1);
        }
        invalidate
    }

    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(super) fn contains_pointer(&self) -> bool {
        self.hovered
    }

    pub(super) fn permits(&self, epoch: u64) -> bool {
        self.hovered && self.epoch == epoch
    }

    pub(super) fn permits_callback(&self, origin: u64, current: Option<u64>) -> bool {
        current == Some(origin) && self.permits(origin)
    }
}

#[cfg(test)]
mod tests {
    use super::Presence;

    #[test]
    fn leave_invalidates_every_tooltip_phase_and_reentry_requires_a_fresh_lifetime() {
        let mut presence = Presence::new(true, true);
        let pending_show = presence.epoch();
        let visible = presence.epoch();
        let pending_hide = presence.epoch();
        assert!(presence.set_hovered(false));
        let exited = presence.epoch();
        // Linux may deliver both MouseExited and hover(false) for one departure.
        assert!(!presence.set_hovered(false));
        assert_eq!(presence.epoch(), exited);
        assert!(!presence.contains_pointer());
        presence.set_hovered(true);
        for old_phase in [pending_show, visible, pending_hide] {
            assert!(!presence.permits(old_phase));
        }
        assert!(presence.permits(presence.epoch()));
    }

    #[test]
    fn focus_loss_invalidates_old_tips_without_blocking_inactive_popup_hover() {
        let mut presence = Presence::new(true, true);
        let old_tip = presence.epoch();
        assert!(presence.set_active(false));
        assert!(!presence.permits(old_tip));
        assert!(presence.permits(presence.epoch()));
        assert!(!presence.set_active(false));
        let current = presence.epoch();
        presence.set_active(true);
        assert!(presence.permits(current));
    }

    #[test]
    fn source_and_hoverable_tooltip_bounds_cannot_bypass_pointer_departure() {
        let mut presence = Presence::new(true, false);
        let epoch = presence.epoch();
        // A hoverable tooltip can transition from its source into its own bounds.
        for (source, tooltip) in [(true, false), (false, true), (true, true)] {
            assert!(presence.permits(epoch) && (source || tooltip));
        }
        presence.set_hovered(false);
        for (source, tooltip) in [(true, false), (false, true), (true, true)] {
            assert!(!(presence.permits(epoch) && (source || tooltip)));
        }
        presence.set_hovered(true);
        assert!(!presence.permits(epoch));
    }

    #[test]
    fn stale_cached_callbacks_and_show_timers_cannot_claim_a_new_pending_show() {
        let mut presence = Presence::new(true, false);
        let old_callback = presence.epoch();
        presence.set_hovered(false);
        presence.set_hovered(true);
        let new_pending_show = Some(presence.epoch());
        assert!(!presence.permits_callback(old_callback, new_pending_show));
        assert!(presence.permits_callback(presence.epoch(), new_pending_show));
        assert!(!presence.permits_callback(presence.epoch(), None));
    }
}
