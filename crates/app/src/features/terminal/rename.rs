//! Names belong to live sessions; an inline edit never changes the shell title.
use crate::{DismissModal, SubmitModal, app::Crabdash, components::text_field::TextField};
use gpui::{prelude::*, *};
use uuid::Uuid;

#[derive(Clone, Copy, Eq, PartialEq)]
struct Target {
    machine: Uuid,
    session: Uuid,
    workspace: Uuid,
    input: EntityId,
    field: EntityId,
}

pub(super) struct Editor {
    target: Target,
    field: Entity<TextField>,
    _blur: Subscription,
}

fn validate_name(text: &str) -> Result<Option<String>, &'static str> {
    if text.chars().any(char::is_control) {
        return Err("Terminal names cannot contain control characters.");
    }
    let name = text.trim();
    if name.chars().count() > 256 {
        return Err("Terminal names must be 256 characters or fewer.");
    }
    Ok((!name.is_empty()).then(|| name.to_owned()))
}

pub(super) fn visible_title(custom_name: Option<&str>, automatic: &str) -> String {
    custom_name.map_or_else(|| automatic.to_owned(), str::to_owned)
}

pub(super) fn tooltip_title(custom_name: Option<&str>, shell: &str) -> String {
    match custom_name {
        Some(name) if name != shell && !shell.is_empty() => format!("{name}\n{shell}"),
        Some(name) if shell.is_empty() => name.to_owned(),
        _ => shell.to_owned(),
    }
}

fn allowed(app: &Crabdash, cx: &App) -> bool {
    app.quake_terminal_open && super::actions::allowed(app, cx) && app.open_menu.is_none()
}

impl Crabdash {
    fn terminal_editor_is_current(&self, target: Target, window: &Window, cx: &App) -> bool {
        allowed(self, cx)
            && self.selected_machine().uuid == target.machine
            && self.workspaces.store.active == target.workspace
            && self
                .quake_terminals
                .get(&target.machine)
                .is_some_and(|drawer| {
                    drawer.model.layout.focused_tab() == Some(target.session)
                        && drawer.sessions.get(&target.session).is_some_and(|session| {
                            session.input.entity_id() == target.input
                                && session.rename.as_ref().is_some_and(|editor| {
                                    editor.target == target
                                        && editor.field.focus_handle(cx).is_focused(window)
                                })
                        })
                })
    }

    pub(super) fn begin_terminal_rename(
        &mut self,
        machine: Uuid,
        session: Uuid,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !allowed(self, cx) || self.selected_machine().uuid != machine {
            return;
        }
        let Some(input) = self
            .quake_terminals
            .get(&machine)
            .and_then(|drawer| drawer.sessions.get(&session))
            .map(|session| session.input.entity_id())
        else {
            return;
        };
        self.select_terminal_session(machine, session, window, cx);
        for drawer in self.quake_terminals.values_mut() {
            for session in drawer.sessions.values_mut() {
                session.rename = None;
            }
        }
        let field = cx.new(|cx| TextField::new("", "Terminal name", 0, cx).compact());
        field.update(cx, |field, cx| {
            field.set_text(title, cx);
            field.select_all_text(cx);
        });
        let target = Target {
            machine,
            session,
            workspace: self.workspaces.store.active,
            input,
            field: field.entity_id(),
        };
        let _blur = cx.on_blur(&field.focus_handle(cx), window, move |app, _, cx| {
            app.cancel_terminal_rename(target, cx);
        });
        let Some(session) = self
            .quake_terminals
            .get_mut(&machine)
            .and_then(|drawer| drawer.sessions.get_mut(&session))
        else {
            return;
        };
        session.rename = Some(Editor {
            target,
            field: field.clone(),
            _blur,
        });
        window.focus(&field.focus_handle(cx));
        let owner = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            owner
                .update(cx, |app, cx| {
                    // Reassert only this edit, and only while it still owns focus.
                    if app.terminal_editor_is_current(target, window, cx) {
                        window.focus(&field.focus_handle(cx));
                    }
                })
                .ok();
        });
        cx.notify();
    }

    fn cancel_terminal_rename(&mut self, target: Target, cx: &mut Context<Self>) {
        if let Some(session) = self
            .quake_terminals
            .get_mut(&target.machine)
            .and_then(|drawer| drawer.sessions.get_mut(&target.session))
            && session
                .rename
                .as_ref()
                .is_some_and(|editor| editor.target == target)
        {
            session.rename = None;
            cx.notify();
        }
    }

    fn finish_terminal_rename(
        &mut self,
        target: Target,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal_editor_is_current(target, window, cx) {
            self.cancel_terminal_rename(target, cx);
            return;
        }
        let Some(session) = self
            .quake_terminals
            .get_mut(&target.machine)
            .and_then(|drawer| drawer.sessions.get_mut(&target.session))
        else {
            return;
        };
        if save && let Some(editor) = &session.rename {
            match validate_name(&editor.field.read(cx).text()) {
                Ok(name) => session.custom_name = name,
                Err(error) => {
                    self.set_status_error(error.to_owned());
                    cx.notify();
                    return;
                }
            }
        }
        session.rename = None;
        window.focus(&session.input.focus_handle(cx));
        cx.notify();
    }

    /// Preserve another control's focus, or repair focus when removing its owner.
    pub(crate) fn reconcile_terminal_rename(&mut self, window: &mut Window, cx: &App) {
        let visible = allowed(self, cx);
        let machine = self.selected_machine().uuid;
        let workspace = self.workspaces.store.active;
        let mut repair_focus = false;
        for (owner, drawer) in &mut self.quake_terminals {
            let focused = drawer.model.layout.focused_tab();
            for (id, session) in &mut drawer.sessions {
                let keep = session.rename.as_ref().is_none_or(|editor| {
                    visible
                        && *owner == machine
                        && editor.target.machine == machine
                        && editor.target.session == *id
                        && editor.target.workspace == workspace
                        && editor.target.input == session.input.entity_id()
                        && focused == Some(*id)
                        && editor.field.focus_handle(cx).is_focused(window)
                });
                if !keep {
                    repair_focus |= session
                        .rename
                        .as_ref()
                        .is_some_and(|editor| editor.field.focus_handle(cx).is_focused(window));
                    session.rename = None;
                }
            }
        }
        if repair_focus {
            window.focus(&self.focus_handle);
        }
    }
}

pub(super) fn render(editor: &Editor, cx: &mut Context<Crabdash>) -> Stateful<Div> {
    let target = editor.target;
    div()
        .id(SharedString::from(format!(
            "terminal-rename-{}",
            target.field
        )))
        .flex_1()
        .min_w_0()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(|_, _, cx| cx.stop_propagation())
        .on_action(cx.listener(move |app, _: &SubmitModal, window, cx| {
            app.finish_terminal_rename(target, true, window, cx);
            cx.stop_propagation();
        }))
        .on_action(cx.listener(move |app, _: &DismissModal, window, cx| {
            app.finish_terminal_rename(target, false, window, cx);
            cx.stop_propagation();
        }))
        .child(editor.field.clone())
}

#[cfg(test)]
mod tests {
    use super::{tooltip_title, validate_name, visible_title};

    #[test]
    fn manual_names_trim_and_blank_names_restore_live_titles() -> Result<(), &'static str> {
        let name = validate_name("  Build · 雪  ")?;
        assert_eq!(name.as_deref(), Some("Build · 雪"));
        assert_eq!(visible_title(name.as_deref(), "~/src"), "Build · 雪");
        assert_eq!(visible_title(name.as_deref(), "~/other"), "Build · 雪");
        assert_eq!(
            tooltip_title(name.as_deref(), "~/other"),
            "Build · 雪\n~/other"
        );
        assert_eq!(tooltip_title(name.as_deref(), "Build · 雪"), "Build · 雪");
        assert_eq!(tooltip_title(name.as_deref(), ""), "Build · 雪");
        let automatic = validate_name(" \u{a0} ")?;
        assert_eq!(automatic, None);
        assert_eq!(visible_title(automatic.as_deref(), "~/other"), "~/other");
        Ok(())
    }

    #[test]
    fn manual_names_reject_controls_and_bound_unicode_characters() -> Result<(), &'static str> {
        for name in ["name\n", "\tname", "a\x1bb", "a\0b"] {
            assert!(validate_name(name).is_err());
        }
        assert_eq!(validate_name(&"界".repeat(256))?, Some("界".repeat(256)));
        assert!(validate_name(&"界".repeat(257)).is_err());
        Ok(())
    }
}
