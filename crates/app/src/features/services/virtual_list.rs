//! Service identity and height reconciliation for GPUI's variable-height list.
//! Visible items are remeasured by GPUI; changes to offscreen items must also
//! invalidate its height cache without moving the user's scroll anchor.
use gpui::{ListAlignment, ListOffset, ListState, px};
use std::{cell::RefCell, ops::Range};
use uuid::Uuid;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Metrics {
    pub compact: bool,
    pub interface_font: String,
    pub interface_size: u32,
    pub log_line_height: u32,
    pub action_width: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Row {
    pub name: String,
    pub description: bool,
    /// The rendered log panel's content height, represented as f32 bits.
    /// None is collapsed; Some includes loading and empty log panels.
    pub logs: Option<u32>,
}

#[derive(Default)]
struct Snapshot {
    target: Option<(Uuid, u64)>,
    metrics: Metrics,
    rows: Vec<Row>,
}

struct Plan {
    reset: bool,
    replacement: Option<(Range<usize>, usize)>,
    invalidate: Vec<usize>,
    anchor: ListOffset,
}

/// The heading is item zero, followed by services and a transparent tail item.
pub(super) struct State {
    pub handle: ListState,
    snapshot: RefCell<Snapshot>,
}

impl State {
    pub fn new() -> Self {
        Self {
            handle: ListState::new(2, ListAlignment::Top, px(128.0)),
            snapshot: RefCell::new(Snapshot::default()),
        }
    }

    pub fn scroll_to_top(&self) {
        self.handle.scroll_to(ListOffset::default());
    }

    pub fn is_scrolled(&self) -> bool {
        let offset = self.handle.logical_scroll_top();
        offset.item_ix > 0 || offset.offset_in_item > px(0.0)
    }

    /// Called before constructing the list callback, never from inside it:
    /// GPUI holds the ListState's mutable borrow while rendering its items.
    pub fn prepare(&self, target: (Uuid, u64), rows: Vec<Row>, metrics: Metrics) {
        let next = Snapshot {
            target: Some(target),
            metrics,
            rows,
        };
        let mut previous = self.snapshot.borrow_mut();
        let plan = reconcile(&previous, &next, self.handle.logical_scroll_top());
        if plan.reset {
            self.handle.reset(next.rows.len() + 2);
        } else {
            let changed = plan.replacement.is_some() || !plan.invalidate.is_empty();
            if let Some((range, count)) = plan.replacement {
                self.handle.splice(range, count);
            }
            for index in plan.invalidate {
                self.handle.splice(index..index + 1, 1);
            }
            if changed {
                self.handle.scroll_to(plan.anchor);
            }
        }
        *previous = next;
    }
}

fn reconcile(previous: &Snapshot, next: &Snapshot, offset: ListOffset) -> Plan {
    if previous.target != next.target {
        return Plan {
            reset: true,
            replacement: None,
            invalidate: Vec::new(),
            anchor: ListOffset::default(),
        };
    }

    let old = &previous.rows;
    let new = &next.rows;
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(a, b)| a.name == b.name)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a.name == b.name)
        .count();
    let replacement = (prefix + suffix < old.len() || prefix + suffix < new.len()).then_some((
        prefix + 1..old.len() - suffix + 1,
        new.len() - prefix - suffix,
    ));

    let metrics_changed = previous.metrics != next.metrics;
    let mut invalidate = Vec::new();
    if metrics_changed {
        invalidate.push(0);
    }
    // These items survive the splice. Replaced items are already unmeasured.
    for (old_index, new_index) in (0..prefix)
        .map(|i| (i, i))
        .chain((0..suffix).map(|i| (old.len() - suffix + i, new.len() - suffix + i)))
    {
        let was_last = old_index + 1 == old.len();
        let is_last = new_index + 1 == new.len();
        if metrics_changed || old[old_index] != new[new_index] || was_last != is_last {
            invalidate.push(new_index + 1);
        }
    }

    Plan {
        reset: false,
        replacement,
        invalidate,
        anchor: retain_anchor(previous, next, offset),
    }
}

fn retain_anchor(previous: &Snapshot, next: &Snapshot, offset: ListOffset) -> ListOffset {
    let old = &previous.rows;
    let new = &next.rows;
    let metrics_changed = previous.metrics != next.metrics;
    if offset.item_ix == 0 {
        return ListOffset {
            offset_in_item: if metrics_changed {
                px(0.0)
            } else {
                offset.offset_in_item
            },
            ..offset
        };
    }
    let old_index = offset.item_ix - 1;
    let Some(row) = old.get(old_index) else {
        // Preserve the trailing spacer when the user is at the list's end.
        return ListOffset {
            item_ix: new.len() + 1,
            ..offset
        };
    };
    if let Some(index) = new.iter().position(|candidate| candidate.name == row.name) {
        // GPUI does not normalize a supplied offset when an item shrinks.
        // An offset inside removed logs can otherwise skip subsequent rows.
        // Preserve the service identity, but return to its beginning whenever
        // its old intra-row offset may no longer be within the new height.
        let may_shrink = metrics_changed
            || (row.description && !new[index].description)
            || logs_shrank(row.logs, new[index].logs)
            || (old_index + 1 == old.len() && index + 1 != new.len());
        return ListOffset {
            item_ix: index + 1,
            offset_in_item: if may_shrink {
                px(0.0)
            } else {
                offset.offset_in_item
            },
        };
    }
    // A disappeared anchor falls forward to the next surviving service, then
    // backward to the nearest previous survivor. No stale row index is reused.
    let survivor = old[old_index + 1..]
        .iter()
        .chain(old[..old_index].iter().rev())
        .find_map(|row| new.iter().position(|candidate| candidate.name == row.name));
    ListOffset {
        item_ix: survivor.map_or(0, |index| index + 1),
        offset_in_item: px(0.0),
    }
}

fn logs_shrank(old: Option<u32>, new: Option<u32>) -> bool {
    match (old, new) {
        (Some(_), None) => true,
        (Some(old), Some(new)) if old != new => {
            let old = f32::from_bits(old);
            let new = f32::from_bits(new);
            !old.is_finite() || !new.is_finite() || new < old
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(names: &[&str]) -> Snapshot {
        Snapshot {
            target: Some((Uuid::nil(), 0)),
            metrics: Metrics::default(),
            rows: names
                .iter()
                .map(|name| Row {
                    name: (*name).into(),
                    description: true,
                    logs: None,
                })
                .collect(),
        }
    }
    fn offset(item_ix: usize) -> ListOffset {
        ListOffset {
            item_ix,
            offset_in_item: px(11.0),
        }
    }

    #[test]
    fn unchanged_sample_keeps_measurements_and_scroll() {
        let old = snapshot(&["a", "b", "c"]);
        let plan = reconcile(&old, &snapshot(&["a", "b", "c"]), offset(2));
        assert!(!plan.reset);
        assert!(plan.replacement.is_none());
        assert!(plan.invalidate.is_empty());
        assert_eq!(plan.anchor.item_ix, 2);
        assert_eq!(plan.anchor.offset_in_item, px(11.0));
    }

    #[test]
    fn insertion_deletion_and_reordering_keep_the_service_anchor() {
        let old = snapshot(&["a", "b", "c"]);
        let inserted = reconcile(&old, &snapshot(&["new", "a", "b", "c"]), offset(2));
        assert_eq!(inserted.replacement, Some((1..1, 1)));
        assert_eq!(inserted.anchor.item_ix, 3);
        assert_eq!(inserted.anchor.offset_in_item, px(11.0));
        let reordered = reconcile(&old, &snapshot(&["c", "a", "b"]), offset(2));
        assert_eq!(reordered.anchor.item_ix, 3);
        assert_eq!(reordered.anchor.offset_in_item, px(11.0));
        let deleted = reconcile(&old, &snapshot(&["b", "c"]), offset(2));
        assert_eq!(deleted.replacement, Some((1..2, 0)));
        assert_eq!(deleted.anchor.item_ix, 1);
    }

    #[test]
    fn disappeared_anchor_selects_nearest_survivor_then_heading() {
        let old = snapshot(&["a", "b", "c", "d"]);
        let next = reconcile(&old, &snapshot(&["a", "d"]), offset(2));
        assert_eq!(next.anchor.item_ix, 2);
        assert_eq!(next.anchor.offset_in_item, px(0.0));
        let previous = reconcile(&old, &snapshot(&["a"]), offset(4));
        assert_eq!(previous.anchor.item_ix, 1);
        let empty = reconcile(&old, &snapshot(&[]), offset(2));
        assert_eq!(empty.anchor.item_ix, 0);
    }

    #[test]
    fn offscreen_logs_and_description_changes_invalidate_only_affected_rows() {
        let old = snapshot(&["a", "b", "c"]);
        let mut loading = snapshot(&["a", "b", "c"]);
        loading.rows[2].logs = Some(18.0_f32.to_bits());
        let plan = reconcile(&old, &loading, offset(1));
        assert_eq!(plan.invalidate, vec![3]);
        let mut ready = snapshot(&["a", "b", "c"]);
        ready.rows[2].logs = Some(300.0_f32.to_bits());
        ready.rows[1].description = false;
        let plan = reconcile(&loading, &ready, offset(1));
        assert_eq!(plan.invalidate, vec![2, 3]);
        assert_eq!(plan.anchor.item_ix, 1);
        assert_eq!(plan.anchor.offset_in_item, px(11.0));
        assert_eq!(reconcile(&ready, &old, offset(1)).invalidate, vec![2, 3]);
    }

    #[test]
    fn top_row_invalidation_restores_offset_after_gpui_splice() {
        let state = State::new();
        let old = snapshot(&["a", "b"]);
        state.prepare((Uuid::nil(), 0), old.rows, old.metrics);
        state.handle.scroll_to(offset(2));
        let mut changed = snapshot(&["a", "b"]);
        changed.rows[1].logs = Some(300.0_f32.to_bits());
        state.prepare((Uuid::nil(), 0), changed.rows, changed.metrics);
        assert_eq!(state.handle.logical_scroll_top().item_ix, 2);
        assert_eq!(state.handle.logical_scroll_top().offset_in_item, px(11.0));
    }

    #[test]
    fn font_and_compact_changes_invalidate_heading_and_all_rows_without_reset() {
        let old = snapshot(&["a", "b"]);
        for metrics in [
            Metrics {
                compact: true,
                ..Metrics::default()
            },
            Metrics {
                interface_size: 16.0_f32.to_bits(),
                ..Metrics::default()
            },
            Metrics {
                interface_font: "Other font".into(),
                ..Metrics::default()
            },
            Metrics {
                log_line_height: 24.0_f32.to_bits(),
                ..Metrics::default()
            },
            Metrics {
                action_width: 120.0_f32.to_bits(),
                ..Metrics::default()
            },
        ] {
            let next = Snapshot {
                metrics,
                ..snapshot(&["a", "b"])
            };
            let plan = reconcile(&old, &next, offset(2));
            assert!(!plan.reset);
            assert_eq!(plan.invalidate, vec![0, 1, 2]);
            assert_eq!(plan.anchor.item_ix, 2);
            assert_eq!(plan.anchor.offset_in_item, px(0.0));
        }
    }

    #[test]
    fn collapsed_and_shortened_anchor_logs_do_not_skip_following_services() {
        let mut old = snapshot(&["a", "b", "c"]);
        old.rows[1].logs = Some(300.0_f32.to_bits());
        let deep = ListOffset {
            item_ix: 2,
            offset_in_item: px(200.0),
        };
        for height in [None, Some(18.0_f32.to_bits())] {
            let mut next = snapshot(&["a", "b", "c"]);
            next.rows[1].logs = height;
            let plan = reconcile(&old, &next, deep);
            assert_eq!(plan.anchor.item_ix, 2);
            assert_eq!(plan.anchor.offset_in_item, px(0.0));
            let state = State::new();
            state.prepare((Uuid::nil(), 0), old.rows.clone(), old.metrics.clone());
            state.handle.scroll_to(deep);
            state.prepare((Uuid::nil(), 0), next.rows, next.metrics);
            assert_eq!(state.handle.logical_scroll_top().item_ix, 2);
            assert_eq!(state.handle.logical_scroll_top().offset_in_item, px(0.0));
        }
    }

    #[test]
    fn growing_anchor_logs_keep_the_exact_intra_row_offset() {
        let mut old = snapshot(&["a", "b"]);
        old.rows[0].logs = Some(18.0_f32.to_bits());
        let mut next = snapshot(&["a", "b"]);
        next.rows[0].logs = Some(300.0_f32.to_bits());
        let plan = reconcile(&old, &next, offset(1));
        assert_eq!(plan.anchor.item_ix, 1);
        assert_eq!(plan.anchor.offset_in_item, px(11.0));
    }

    #[test]
    fn font_and_compact_shrink_reset_deep_offsets_and_preserve_identity() {
        let mut old = snapshot(&["a", "b"]);
        old.metrics.compact = true;
        old.metrics.interface_size = 24.0_f32.to_bits();
        let deep = ListOffset {
            item_ix: 2,
            offset_in_item: px(200.0),
        };
        for metrics in [
            Metrics {
                interface_size: 10.0_f32.to_bits(),
                ..old.metrics.clone()
            },
            Metrics {
                compact: false,
                ..old.metrics.clone()
            },
        ] {
            let next = Snapshot {
                metrics,
                ..snapshot(&["a", "b"])
            };
            let plan = reconcile(&old, &next, deep);
            assert_eq!(plan.anchor.item_ix, 2);
            assert_eq!(plan.anchor.offset_in_item, px(0.0));
        }
        let heading = reconcile(&old, &snapshot(&["a", "b"]), offset(0));
        assert_eq!(heading.anchor.item_ix, 0);
        assert_eq!(heading.anchor.offset_in_item, px(0.0));
    }

    #[test]
    fn machine_changes_and_explicit_filter_search_sort_resets_start_at_heading() {
        let state = State::new();
        let old = snapshot(&["a", "b"]);
        state.prepare((Uuid::nil(), 0), old.rows, old.metrics);
        state.handle.scroll_to(offset(2));
        let next = snapshot(&["a", "b"]);
        state.prepare((Uuid::nil(), 1), next.rows, next.metrics);
        assert_eq!(state.handle.logical_scroll_top().item_ix, 0);
        assert_eq!(state.handle.logical_scroll_top().offset_in_item, px(0.0));
        for names in [&["b"][..], &["b", "a"][..], &["a", "b"][..]] {
            state.handle.scroll_to(offset(1));
            state.scroll_to_top();
            let next = snapshot(names);
            state.prepare((Uuid::nil(), 1), next.rows, next.metrics);
            assert_eq!(state.handle.logical_scroll_top().item_ix, 0);
            assert_eq!(state.handle.logical_scroll_top().offset_in_item, px(0.0));
        }
        state.handle.scroll_to(offset(1));
        let next = snapshot(&["a", "b"]);
        state.prepare((Uuid::from_u128(1), 1), next.rows, next.metrics);
        assert_eq!(state.handle.logical_scroll_top().item_ix, 0);
    }

    #[test]
    fn appended_row_invalidates_previous_last_border_and_tail_anchor_moves() {
        let old = snapshot(&["a", "b"]);
        let plan = reconcile(&old, &snapshot(&["a", "b", "c"]), offset(3));
        assert_eq!(plan.replacement, Some((3..3, 1)));
        assert_eq!(plan.invalidate, vec![2]);
        assert_eq!(plan.anchor.item_ix, 4);
    }

    #[test]
    fn large_inventory_remains_complete_while_measurements_are_lazy() {
        let state = State::new();
        let rows = (0..228)
            .map(|index| Row {
                name: format!("unit-{index}.service"),
                description: true,
                logs: None,
            })
            .collect();
        state.prepare((Uuid::nil(), 0), rows, Metrics::default());
        assert_eq!(state.handle.item_count(), 230);
        state.handle.scroll_to(offset(228));
        let unchanged = state.snapshot.borrow().rows.clone();
        state.prepare((Uuid::nil(), 0), unchanged, Metrics::default());
        assert_eq!(state.handle.logical_scroll_top().item_ix, 228);
    }
}
