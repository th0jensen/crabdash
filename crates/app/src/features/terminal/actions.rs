//! Session shortcuts follow actual input focus and share menu availability.
use crate::{app::Crabdash, features::preferences};
use gpui::{prelude::*, *};
use uuid::Uuid;

actions!(
    crabdash_terminal,
    [
        NewSession,
        CloseSession,
        NextTab,
        PreviousTab,
        SplitRight,
        SplitDown,
        FocusLeft,
        FocusRight,
        FocusUp,
        FocusDown
    ]
);

pub(crate) fn bind_keys(cx: &mut App) {
    super::selection::bind_keys(cx);
    use crate::desktop::menus::shortcut;
    cx.bind_keys([
        KeyBinding::new(shortcut("cmd-t", "ctrl-shift-t"), NewSession, None),
        KeyBinding::new(
            shortcut("cmd-shift-w", "ctrl-shift-w"),
            CloseSession,
            Some("CrabdashTerminalInput"),
        ),
        KeyBinding::new(
            shortcut("alt-cmd-right", "ctrl-pagedown"),
            NextTab,
            Some("CrabdashTerminalInput"),
        ),
        KeyBinding::new(
            shortcut("alt-cmd-left", "ctrl-pageup"),
            PreviousTab,
            Some("CrabdashTerminalInput"),
        ),
        KeyBinding::new(
            shortcut("cmd-d", "ctrl-shift-d"),
            SplitRight,
            Some("CrabdashTerminalInput"),
        ),
        KeyBinding::new(
            shortcut("cmd-shift-d", "ctrl-shift-e"),
            SplitDown,
            Some("CrabdashTerminalInput"),
        ),
        KeyBinding::new(
            shortcut("ctrl-cmd-left", "ctrl-shift-left"),
            FocusLeft,
            Some("CrabdashTerminalInput"),
        ),
        KeyBinding::new(
            shortcut("ctrl-cmd-right", "ctrl-shift-right"),
            FocusRight,
            Some("CrabdashTerminalInput"),
        ),
        KeyBinding::new(
            shortcut("ctrl-cmd-up", "ctrl-shift-up"),
            FocusUp,
            Some("CrabdashTerminalInput"),
        ),
        KeyBinding::new(
            shortcut("ctrl-cmd-down", "ctrl-shift-down"),
            FocusDown,
            Some("CrabdashTerminalInput"),
        ),
    ]);
}

pub(super) fn allowed(app: &Crabdash, cx: &App) -> bool {
    !app.startup_busy
        && !app.preferences_open
        && !app.preference_editor.busy
        && !preferences::mutation::is_busy(cx)
        && app.machine_rename.target.is_none()
        && !app.machine_rename.busy
        && !app.add_machine_modal_open
        && !app.docker_run_modal_open
        && !app.docker_run_config.busy
        && app.docker_removal.is_none()
        && !app.workspaces.open
        && !cx.has_active_drag()
}

pub(super) fn focused_session(app: &Crabdash, window: &Window, cx: &App) -> Option<(Uuid, Uuid)> {
    if !app.quake_terminal_open {
        return None;
    }
    let machine = app.selected_machine().uuid;
    let drawer = app.quake_terminals.get(&machine)?;
    drawer
        .sessions
        .iter()
        .find(|(_, session)| session.input.focus_handle(cx).is_focused(window))
        .map(|(id, _)| (machine, *id))
}

fn adjacent(app: &Crabdash, window: &Window, cx: &App, forward: bool) -> Option<(Uuid, Uuid)> {
    let (machine, focused) = focused_session(app, window, cx)?;
    let drawer = app.quake_terminals.get(&machine)?;
    if drawer.model.layout.focused_tab() != Some(focused) {
        return None;
    }
    drawer
        .model
        .adjacent(forward)
        .map(|session| (machine, session))
}

fn focused_drawer<'a>(app: &'a Crabdash, window: &Window, cx: &App) -> Option<&'a super::Drawer> {
    let (machine, session) = focused_session(app, window, cx)?;
    app.quake_terminals
        .get(&machine)
        .filter(|drawer| drawer.model.layout.focused_tab() == Some(session))
}

fn can_split(app: &Crabdash, window: &Window, cx: &App, edge: crate::layout::Drop) -> bool {
    focused_drawer(app, window, cx).is_some_and(|drawer| {
        drawer
            .model
            .with_session(Uuid::new_v4(), Some(edge))
            .is_some()
    })
}

fn split(
    app: &mut Crabdash,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
    edge: crate::layout::Drop,
) {
    if allowed(app, cx) && can_split(app, window, cx, edge) {
        app.open_menu = None;
        app.split_quake_terminal(edge, window, cx);
    }
}

fn neighbor(
    app: &Crabdash,
    window: &Window,
    cx: &App,
    direction: super::panes::Direction,
) -> Option<Uuid> {
    focused_drawer(app, window, cx)?;
    app.terminal_neighbor(direction, window, cx)
}

fn focus(
    app: &mut Crabdash,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
    direction: super::panes::Direction,
) {
    if allowed(app, cx)
        && let Some(session) = neighbor(app, window, cx, direction)
    {
        app.open_menu = None;
        app.select_terminal_session(app.selected_machine().uuid, session, window, cx);
    }
}

fn navigate(app: &mut Crabdash, window: &mut Window, cx: &mut Context<Crabdash>, forward: bool) {
    if allowed(app, cx)
        && let Some((machine, session)) = adjacent(app, window, cx, forward)
    {
        app.open_menu = None;
        app.select_terminal_session(machine, session, window, cx);
    }
}

pub(crate) fn decorate(
    root: Div,
    app: &Crabdash,
    window: &Window,
    cx: &mut Context<Crabdash>,
) -> Div {
    let root = super::selection::decorate(root, app, window, cx);
    let enabled = allowed(app, cx);
    root.when(enabled, |root| {
        root.on_action(cx.listener(|app, _: &NewSession, window, cx| {
            if allowed(app, cx) {
                app.open_menu = None;
                app.new_quake_terminal(window, cx);
                app.persist_workspace(cx);
            }
        }))
    })
    .when(
        enabled && focused_session(app, window, cx).is_some(),
        |root| {
            root.on_action(cx.listener(|app, _: &CloseSession, window, cx| {
                if allowed(app, cx)
                    && let Some((machine, session)) = focused_session(app, window, cx)
                {
                    app.open_menu = None;
                    app.close_terminal_session(machine, session, window, cx);
                }
            }))
        },
    )
    .when(
        enabled && adjacent(app, window, cx, true).is_some(),
        |root| {
            root.on_action(
                cx.listener(|app, _: &NextTab, window, cx| navigate(app, window, cx, true)),
            )
            .on_action(
                cx.listener(|app, _: &PreviousTab, window, cx| navigate(app, window, cx, false)),
            )
        },
    )
    .when(
        enabled && can_split(app, window, cx, crate::layout::Drop::Right),
        |root| {
            root.on_action(cx.listener(|app, _: &SplitRight, window, cx| {
                split(app, window, cx, crate::layout::Drop::Right)
            }))
        },
    )
    .when(
        enabled && can_split(app, window, cx, crate::layout::Drop::Bottom),
        |root| {
            root.on_action(cx.listener(|app, _: &SplitDown, window, cx| {
                split(app, window, cx, crate::layout::Drop::Bottom)
            }))
        },
    )
    .when(
        enabled && neighbor(app, window, cx, super::panes::Direction::Left).is_some(),
        |root| {
            root.on_action(cx.listener(|app, _: &FocusLeft, window, cx| {
                focus(app, window, cx, super::panes::Direction::Left)
            }))
        },
    )
    .when(
        enabled && neighbor(app, window, cx, super::panes::Direction::Right).is_some(),
        |root| {
            root.on_action(cx.listener(|app, _: &FocusRight, window, cx| {
                focus(app, window, cx, super::panes::Direction::Right)
            }))
        },
    )
    .when(
        enabled && neighbor(app, window, cx, super::panes::Direction::Up).is_some(),
        |root| {
            root.on_action(cx.listener(|app, _: &FocusUp, window, cx| {
                focus(app, window, cx, super::panes::Direction::Up)
            }))
        },
    )
    .when(
        enabled && neighbor(app, window, cx, super::panes::Direction::Down).is_some(),
        |root| {
            root.on_action(cx.listener(|app, _: &FocusDown, window, cx| {
                focus(app, window, cx, super::panes::Direction::Down)
            }))
        },
    )
}
