//! Resource status changes independently of collector completion and keyboard focus.
use super::MachineState;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Status {
    Starting,
    Live { interval: Duration },
    Waiting,
    Unavailable,
    SampleFailed,
}

impl Status {
    pub(super) fn label(self) -> String {
        match self {
            Self::Starting => "Starting…".into(),
            Self::Live { interval } => format!("Live · {}s", interval.as_secs()),
            Self::Waiting => "Waiting for a sample…".into(),
            Self::Unavailable => "Unavailable".into(),
            Self::SampleFailed => "Sample failed · showing last readings".into(),
        }
    }

    pub(super) fn empty_message(self) -> &'static str {
        match self {
            Self::Waiting => "Waiting for the first resource sample…",
            Self::Unavailable | Self::SampleFailed => "Resource samples are unavailable. Retrying…",
            Self::Starting | Self::Live { .. } => "Collecting resource samples…",
        }
    }
}

#[derive(Default)]
struct Snapshot {
    updated: Option<Instant>,
    requested: Option<Instant>,
    has_readings: bool,
    failed: bool,
}

impl Snapshot {
    fn status(&self, now: Instant, interval: Duration) -> Status {
        // A retry does not erase the last failure or make retained readings live.
        if self.failed {
            return if self.has_readings {
                Status::SampleFailed
            } else {
                Status::Unavailable
            };
        }
        let stale_after = interval.saturating_mul(3);
        if self.has_readings
            && let Some(updated) = self.updated
        {
            return if now.saturating_duration_since(updated) >= stale_after {
                Status::Waiting
            } else {
                Status::Live { interval }
            };
        }
        if self
            .requested
            .is_some_and(|requested| now.saturating_duration_since(requested) >= stale_after)
        {
            Status::Waiting
        } else {
            Status::Starting
        }
    }
}

pub(super) fn current(state: Option<&MachineState>, now: Instant, interval: Duration) -> Status {
    state
        .map_or_else(Snapshot::default, |state| Snapshot {
            updated: state.updated,
            requested: state.last_requested,
            has_readings: state.usage.is_some(),
            failed: state.error.is_some(),
        })
        .status(now, interval)
}

/// Remember the status prepared for rendering, rather than comparing two reads
/// of elapsed time on the same heartbeat. Hidden panes never request a redraw.
#[derive(Default)]
pub(super) struct Tracker {
    previous: Option<(Uuid, Status)>,
}

impl Tracker {
    pub(super) fn observe(&mut self, current: Option<(Uuid, Status)>) -> bool {
        let changed = self.previous != current;
        self.previous = current;
        current.is_some() && changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_slow_request_changes_from_starting_to_waiting_at_the_boundary() {
        let start = Instant::now();
        let interval = Duration::from_secs(2);
        assert_eq!(current(None, start, interval), Status::Starting);
        let pending = Snapshot {
            requested: Some(start),
            ..Default::default()
        };
        assert_eq!(
            pending.status(start + Duration::from_millis(5999), interval),
            Status::Starting
        );
        assert_eq!(
            pending.status(start + Duration::from_secs(6), interval),
            Status::Waiting
        );
        assert_eq!(
            pending.status(start + Duration::from_secs(60), interval),
            Status::Waiting
        );
        assert_eq!(
            Status::Waiting.empty_message(),
            "Waiting for the first resource sample…"
        );
    }

    #[test]
    fn new_requests_do_not_make_old_readings_fresh() {
        let start = Instant::now();
        let interval = Duration::from_secs(2);
        let pending = Snapshot {
            updated: Some(start),
            requested: Some(start + Duration::from_secs(5)),
            has_readings: true,
            failed: false,
        };
        assert_eq!(
            pending.status(start + Duration::from_millis(5999), interval),
            Status::Live { interval }
        );
        assert_eq!(
            pending.status(start + Duration::from_secs(6), interval),
            Status::Waiting
        );
    }

    #[test]
    fn failures_stay_explicit_during_retries_and_success_restores_live_status() {
        let start = Instant::now();
        let now = start + Duration::from_secs(20);
        let interval = Duration::from_secs(2);
        let mut sample = Snapshot {
            requested: Some(now),
            failed: true,
            ..Default::default()
        };
        assert_eq!(sample.status(now, interval), Status::Unavailable);
        assert_eq!(
            sample.status(now + Duration::from_secs(10), interval),
            Status::Unavailable
        );
        assert_eq!(
            Status::Unavailable.empty_message(),
            "Resource samples are unavailable. Retrying…"
        );
        sample.updated = Some(start);
        sample.has_readings = true;
        assert_eq!(sample.status(now, interval), Status::SampleFailed);
        assert_eq!(
            Status::SampleFailed.label(),
            "Sample failed · showing last readings"
        );
        sample.failed = false;
        sample.updated = Some(now);
        assert_eq!(sample.status(now, interval), Status::Live { interval });
    }

    #[test]
    fn interval_changes_recompute_freshness_and_live_cadence() {
        let start = Instant::now();
        let now = start + Duration::from_secs(9);
        let sample = Snapshot {
            updated: Some(start),
            has_readings: true,
            ..Default::default()
        };
        assert_eq!(sample.status(now, Duration::from_secs(2)), Status::Waiting);
        let live = sample.status(now, Duration::from_secs(4));
        assert_eq!(
            live,
            Status::Live {
                interval: Duration::from_secs(4)
            }
        );
        assert_eq!(live.label(), "Live · 4s");
        let pending = Snapshot {
            requested: Some(start),
            ..Default::default()
        };
        assert_eq!(pending.status(now, Duration::from_secs(2)), Status::Waiting);
        assert_eq!(
            pending.status(now, Duration::from_secs(4)),
            Status::Starting
        );
    }

    #[test]
    fn visible_status_transitions_notify_once_and_hidden_panes_never_notify() {
        let machine = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        let live = Status::Live {
            interval: Duration::from_secs(2),
        };
        let mut tracker = Tracker::default();
        // Preparation records the initial render's status; ordinary heartbeats
        // and pending requests then leave the unchanged mosaic alone.
        assert!(tracker.observe(Some((machine, live))));
        assert!(!tracker.observe(Some((machine, live))));
        assert!(tracker.observe(Some((machine, Status::Waiting))));
        assert!(!tracker.observe(Some((machine, Status::Waiting))));
        assert!(!tracker.observe(None));
        assert!(!tracker.observe(None));
        assert!(tracker.observe(Some((machine, Status::Waiting))));
        assert!(tracker.observe(Some((other, Status::Waiting))));
        assert!(tracker.observe(Some((other, live))));
        assert!(tracker.observe(Some((
            other,
            Status::Live {
                interval: Duration::from_secs(4)
            }
        ))));
        assert!(!tracker.observe(Some((
            other,
            Status::Live {
                interval: Duration::from_secs(4)
            }
        ))));
    }

    #[test]
    fn future_timestamps_and_large_intervals_do_not_panic_or_invent_staleness() {
        let now = Instant::now();
        let future = Snapshot {
            updated: Some(now + Duration::from_secs(1)),
            has_readings: true,
            ..Default::default()
        };
        let interval = Duration::from_secs(u64::MAX);
        assert_eq!(future.status(now, interval), Status::Live { interval });
    }
}
