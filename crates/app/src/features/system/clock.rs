//! A heartbeat guard invalidates samples after sleep, clock steps, or UI stalls.
//! Polling cadence is independent of the configured resource sampling interval.
use std::time::{Duration, Instant, SystemTime};

const MAX_HEARTBEAT_GAP: Duration = Duration::from_secs(5);
const MAX_CLOCK_DRIFT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
pub(super) struct Reading {
    pub instant: Instant,
    pub wall: SystemTime,
}
impl Reading {
    pub fn now() -> Self {
        Self {
            instant: Instant::now(),
            wall: SystemTime::now(),
        }
    }
}

pub(super) struct SamplingClock {
    previous: Reading,
}
impl Default for SamplingClock {
    fn default() -> Self {
        Self::new(Reading::now())
    }
}
impl SamplingClock {
    pub fn new(previous: Reading) -> Self {
        Self { previous }
    }

    /// Called on every heartbeat and before consuming asynchronous results.
    /// Neither Rust clock guarantees consistent suspend behavior on every OS.
    pub fn observe(&mut self, now: Reading) -> bool {
        let previous = std::mem::replace(&mut self.previous, now);
        let Some(monotonic) = now.instant.checked_duration_since(previous.instant) else {
            return true;
        };
        let Ok(wall) = now.wall.duration_since(previous.wall) else {
            return true;
        };
        monotonic > MAX_HEARTBEAT_GAP
            || wall > MAX_HEARTBEAT_GAP
            || wall.abs_diff(monotonic) > MAX_CLOCK_DRIFT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_keeps_long_sample_intervals_and_slow_requests_valid() {
        let start = Reading::now();
        let mut clock = SamplingClock::new(start);
        for second in 1..=120 {
            assert!(!clock.observe(Reading {
                instant: start.instant + Duration::from_secs(second),
                wall: start.wall + Duration::from_secs(second),
            }));
        }
    }

    #[test]
    fn interruptions_are_detected_whether_monotonic_time_counts_sleep_or_not() {
        let start = Reading::now();
        for (monotonic, wall) in [(1, 600), (600, 600), (6, 6), (1, 4)] {
            let mut clock = SamplingClock::new(start);
            let resumed = Reading {
                instant: start.instant + Duration::from_secs(monotonic),
                wall: start.wall + Duration::from_secs(wall),
            };
            assert!(clock.observe(resumed));
            // Recovery moves the checkpoint, producing one interruption.
            assert!(!clock.observe(Reading {
                instant: resumed.instant + Duration::from_secs(1),
                wall: resumed.wall + Duration::from_secs(1),
            }));
        }
    }

    #[test]
    fn reversed_clocks_reset_and_small_scheduler_jitter_is_tolerated() {
        let start = Reading::now();
        let mut clock = SamplingClock::new(start);
        assert!(clock.observe(Reading {
            instant: start.instant + Duration::from_secs(1),
            wall: start.wall - Duration::from_secs(1),
        }));
        let mut clock = SamplingClock::new(start);
        assert!(clock.observe(Reading {
            instant: start.instant - Duration::from_secs(1),
            wall: start.wall + Duration::from_secs(1),
        }));
        let mut clock = SamplingClock::new(start);
        assert!(!clock.observe(Reading {
            instant: start.instant + Duration::from_millis(1400),
            wall: start.wall + Duration::from_millis(1700),
        }));
    }
}
