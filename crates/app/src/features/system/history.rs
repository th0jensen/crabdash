//! Bounded, timestamped histories preserve unknown measurements as gaps.
use std::{collections::VecDeque, time::Instant};

pub(super) const HISTORY_LIMIT: usize = 60;

#[derive(Clone, Copy)]
pub(crate) struct ScalarPoint {
    pub captured_at: Instant,
    pub value: Option<f64>,
}

#[derive(Clone, Copy)]
pub(crate) struct HistoryPoint {
    pub captured_at: Instant,
    pub cpu: Option<f64>,
    pub memory: Option<f64>,
    pub network_rx: Option<f64>,
    pub network_tx: Option<f64>,
    pub disk_read: Option<f64>,
    pub disk_write: Option<f64>,
}
impl HistoryPoint {
    pub(super) fn gap(captured_at: Instant) -> Self {
        Self {
            captured_at,
            cpu: None,
            memory: None,
            network_rx: None,
            network_tx: None,
            disk_read: None,
            disk_write: None,
        }
    }
}

pub(super) fn append<T>(history: &mut VecDeque<T>, value: T) {
    history.push_back(value);
    while history.len() > HISTORY_LIMIT {
        history.pop_front();
    }
}
