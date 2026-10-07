//! Inline renames compare the target's original name, not unrelated revisions.
use super::store::{Store, validate_name};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Draft {
    pub(super) id: Uuid,
    pub(super) original_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Conflict {
    Changed,
    Deleted,
}
impl Conflict {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Changed => "Name changed in another window. Press Escape, then rename again.",
            Self::Deleted => "The workspace being renamed was deleted in another window.",
        }
    }
}

#[derive(Debug)]
pub(super) enum Failure {
    Conflict(Conflict),
    InvalidName(String),
}

fn check(store: &Store, draft: &Draft) -> Result<usize, Conflict> {
    let index = store
        .workspaces
        .iter()
        .position(|workspace| workspace.id == draft.id)
        .ok_or(Conflict::Deleted)?;
    if store.workspaces[index].name != draft.original_name {
        return Err(Conflict::Changed);
    }
    Ok(index)
}

pub(super) fn reconcile(store: &Store, draft: &mut Option<Draft>) -> Option<Conflict> {
    let conflict = check(store, draft.as_ref()?).err()?;
    if conflict == Conflict::Deleted {
        *draft = None;
    }
    Some(conflict)
}

pub(super) fn commit(store: &mut Store, draft: &Draft, name: &str) -> Result<(), Failure> {
    let index = check(store, draft).map_err(Failure::Conflict)?;
    // Trim surrounding spacing while retaining control characters for the
    // shared validator to reject, including pasted newlines at the edges.
    let name =
        name.trim_matches(|character: char| character.is_whitespace() && !character.is_control());
    validate_name(name).map_err(|error| Failure::InvalidName(error.to_string()))?;
    store.workspaces[index].name = name.to_string();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::workspaces::model::{Drop, Tab};

    fn fixture() -> (Store, Draft) {
        let mut store = Store::default();
        let draft = Draft {
            id: store.active,
            original_name: store.current().name.clone(),
        };
        let mut second = store.current().clone();
        second.id = Uuid::new_v4();
        second.name = "Other".into();
        store.workspaces.push(second);
        (store, draft)
    }

    #[test]
    fn unrelated_layout_focus_sidebar_and_active_workspace_changes_preserve_draft() {
        let (mut store, draft) = fixture();
        assert!(
            store
                .current_mut()
                .layout
                .drop_tab(Tab::Services, 1, Drop::Right)
        );
        assert!(store.current_mut().layout.focus(1));
        store.current_mut().sidebar_width = 300.0;
        store.current_mut().sidebar_collapsed = true;
        store.current_mut().terminal_open = true;
        store.active = store.workspaces[1].id;
        let mut editing = Some(draft.clone());
        assert_eq!(reconcile(&store, &mut editing), None);
        assert_eq!(editing, Some(draft));
    }

    #[test]
    fn another_windows_rename_rejects_repeated_saves_without_mutation_until_reopened()
    -> anyhow::Result<()> {
        let (mut store, draft) = fixture();
        store.workspaces[0].name = "Changed elsewhere".into();
        let before = serde_json::to_value(&store)?;
        let mut editing = Some(draft.clone());
        assert_eq!(reconcile(&store, &mut editing), Some(Conflict::Changed));
        assert_eq!(editing, Some(draft.clone()));
        for _ in 0..2 {
            assert!(matches!(
                commit(&mut store, &draft, "Stale field text"),
                Err(Failure::Conflict(Conflict::Changed))
            ));
            assert_eq!(serde_json::to_value(&store)?, before);
        }
        let reopened = Draft {
            id: draft.id,
            original_name: store.workspaces[0].name.clone(),
        };
        commit(&mut store, &reopened, "Fresh rename")
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert_eq!(store.workspaces[0].name, "Fresh rename");
        Ok(())
    }

    #[test]
    fn target_name_returning_to_original_preserves_and_unblocks_the_draft() -> anyhow::Result<()> {
        let (mut store, draft) = fixture();
        let mut editing = Some(draft.clone());
        store.workspaces[0].name = "Changed elsewhere".into();
        assert_eq!(reconcile(&store, &mut editing), Some(Conflict::Changed));
        store.workspaces[0].name = draft.original_name.clone();
        assert_eq!(reconcile(&store, &mut editing), None);
        assert_eq!(editing, Some(draft.clone()));
        commit(&mut store, &draft, "Preserved draft")
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert_eq!(store.workspaces[0].name, "Preserved draft");
        Ok(())
    }

    #[test]
    fn deleted_target_cancels_draft_and_cannot_commit_a_stale_field() -> anyhow::Result<()> {
        let (mut store, draft) = fixture();
        store.active = store.workspaces[1].id;
        store
            .workspaces
            .retain(|workspace| workspace.id != draft.id);
        let before = serde_json::to_value(&store)?;
        let mut editing = Some(draft.clone());
        assert_eq!(reconcile(&store, &mut editing), Some(Conflict::Deleted));
        assert!(editing.is_none());
        assert_eq!(reconcile(&store, &mut editing), None);
        assert!(matches!(
            commit(&mut store, &draft, "Deleted target"),
            Err(Failure::Conflict(Conflict::Deleted))
        ));
        assert_eq!(serde_json::to_value(&store)?, before);
        store.validate()?;
        Ok(())
    }

    #[test]
    fn invalid_names_preserve_store_and_draft_including_unicode_character_limit()
    -> anyhow::Result<()> {
        let (mut store, draft) = fixture();
        let before = serde_json::to_value(&store)?;
        for name in [
            " ".into(),
            "a\nb".into(),
            "\nname".into(),
            "name\t".into(),
            "界".repeat(49),
        ] {
            assert!(matches!(
                commit(&mut store, &draft, &name),
                Err(Failure::InvalidName(_))
            ));
            assert_eq!(serde_json::to_value(&store)?, before);
            let mut editing = Some(draft.clone());
            assert_eq!(reconcile(&store, &mut editing), None);
            assert_eq!(editing, Some(draft.clone()));
        }
        Ok(())
    }

    #[test]
    fn successful_rename_changes_only_named_target_without_structural_mutations()
    -> anyhow::Result<()> {
        let (mut store, _) = fixture();
        assert!(
            store
                .current_mut()
                .layout
                .drop_tab(Tab::System, 1, Drop::Bottom)
        );
        let draft = Draft {
            id: store.workspaces[1].id,
            original_name: store.workspaces[1].name.clone(),
        };
        let mut expected = serde_json::to_value(&store)?;
        expected["workspaces"][1]["name"] = serde_json::json!("界".repeat(48));
        commit(&mut store, &draft, &format!("  {}  ", "界".repeat(48)))
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        assert_eq!(serde_json::to_value(&store)?, expected);
        store.validate()?;
        Ok(())
    }
}
