//! Native event and editor synchronization policy, independent of AppKit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stamp {
    pub geometry: u64,
    pub editing: u64,
    pub sequence: u64,
    pub model: u64,
}
pub(super) enum Kind {
    Click,
    Focus,
    Edit,
    Choice,
}
pub(super) fn accepts(current: Stamp, queued: Stamp, kind: Kind, model_revision: u64) -> bool {
    if current.editing != queued.editing {
        return false;
    }
    match kind {
        Kind::Click => current.geometry == queued.geometry,
        Kind::Focus => model_revision == queued.model,
        Kind::Edit => current.sequence == queued.sequence && model_revision == queued.model,
        Kind::Choice => {
            current.geometry == queued.geometry
                && current.sequence == queued.sequence
                && model_revision == queued.model
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sync {
    Unchanged,
    NativeEcho,
    Replace,
}
pub(super) fn synchronize(
    value: &str,
    previous: Option<&str>,
    native_echo: Option<&str>,
    model_revision: u64,
    previous_revision: u64,
) -> Sync {
    if model_revision != previous_revision {
        Sync::Replace
    } else if previous == Some(value) {
        Sync::Unchanged
    } else if native_echo == Some(value) {
        Sync::NativeEcho
    } else {
        Sync::Replace
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn initial() -> Stamp {
        Stamp {
            geometry: 1,
            editing: 1,
            sequence: 1,
            model: 0,
        }
    }
    #[test]
    fn choice_requires_current_geometry_context_model_and_latest_selection() {
        let first = initial();
        let latest = Stamp {
            sequence: 2,
            ..first
        };
        assert!(!accepts(latest, first, Kind::Choice, 0));
        assert!(accepts(latest, latest, Kind::Choice, 0));
        assert!(!accepts(latest, latest, Kind::Choice, 1));
        assert!(!accepts(
            Stamp {
                geometry: 2,
                ..latest
            },
            latest,
            Kind::Choice,
            0
        ));
        assert!(!accepts(
            Stamp {
                editing: 2,
                ..latest
            },
            latest,
            Kind::Choice,
            0
        ));
    }
    #[test]
    fn resized_click_rejected_but_current_input_edit_and_focus_survive() {
        let queued = initial();
        let current = Stamp {
            geometry: 2,
            ..queued
        };
        assert!(!accepts(current, queued, Kind::Click, 0));
        assert!(accepts(current, queued, Kind::Edit, 0));
        assert!(accepts(current, queued, Kind::Focus, 0));
        assert!(!accepts(
            Stamp {
                editing: 2,
                ..current
            },
            queued,
            Kind::Edit,
            0
        ));
    }
    #[test]
    fn typing_coalesces_but_two_current_button_activations_remain_distinct() {
        let first = initial();
        let second = Stamp {
            sequence: 2,
            ..first
        };
        assert!(!accepts(second, first, Kind::Edit, 0));
        assert!(accepts(second, second, Kind::Edit, 0));
        assert!(accepts(second, first, Kind::Click, 0));
    }
    #[test]
    fn same_value_clear_supersedes_pending_typing_before_any_redraw() {
        let queued = initial();
        assert!(!accepts(queued, queued, Kind::Edit, 1));
        assert_eq!(synchronize("", Some(""), None, 1, 0), Sync::Replace);
    }
    #[test]
    fn native_echo_preserves_newer_editor_text_but_programmatic_replacement_wins() {
        assert_eq!(
            synchronize("a", Some(""), Some("a"), 0, 0),
            Sync::NativeEcho
        );
        assert_eq!(
            synchronize("ab", Some("a"), Some("ab"), 0, 0),
            Sync::NativeEcho
        );
        // Assigning an old echoed value deliberately must not be mistaken for
        // another acknowledgement of that historic native edit.
        assert_eq!(synchronize("a", Some("ab"), Some("a"), 1, 0), Sync::Replace);
    }
}
