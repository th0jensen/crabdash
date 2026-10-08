//! Focus-scoped pointer selection and clipboard commands for terminal panes.
mod emulator;
mod pointer;
mod shortcuts;
pub(super) use pointer::layer;
use pointer::{Grid, Held};
#[cfg(test)]
mod tests;

use crate::{app::Crabdash, components::text_field::FieldCopy};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct State {
    stream: Option<emulator::Stream>,
    geometry: Rc<Cell<Option<Grid>>>,
    held: Option<Held>,
    autoscroll: Option<Task<()>>,
}

fn focused(app: &Crabdash, window: &Window, cx: &App) -> Option<(Uuid, Uuid)> {
    if !super::actions::allowed(app, cx) {
        return None;
    }
    let (machine, session) = super::actions::focused_session(app, window, cx)?;
    let drawer = app.quake_terminals.get(&machine)?;
    (drawer.model.layout.focused_tab() == Some(session)
        && drawer.sessions.get(&session)?.terminal.has_selection())
    .then_some((machine, session))
}

fn copy(app: &mut Crabdash, window: &Window, cx: &mut Context<Crabdash>) {
    let Some((machine, session)) = focused(app, window, cx) else {
        return;
    };
    let result = app
        .quake_terminals
        .get(&machine)
        .and_then(|drawer| drawer.sessions.get(&session))
        .map(|session| session.terminal.selected_text());
    match result {
        Some(Ok(Some(text))) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
        Some(Err(error)) => {
            app.set_status_error(format!("Unable to copy terminal text: {error}"));
            cx.notify();
        }
        _ => {}
    }
}

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys(
        shortcuts::copy_keys()
            .iter()
            .map(|key| KeyBinding::new(*key, FieldCopy, Some("CrabdashTerminalInput"))),
    );
}

pub(super) fn decorate(
    root: Div,
    app: &Crabdash,
    _window: &Window,
    cx: &mut Context<Crabdash>,
) -> Div {
    let root = root.when(
        app.quake_terminal_open && super::actions::allowed(app, cx),
        |root| {
            root.on_action(cx.listener(|app, _: &FieldCopy, window, cx| {
                if focused(app, window, cx).is_some() {
                    copy(app, window, cx);
                } else {
                    cx.propagate();
                }
            }))
        },
    );
    let root = root.capture_action(cx.listener(
        |app, _: &crate::components::terminal_input::TerminalInterrupt, window, cx| {
            if shortcuts::interrupt_copies(focused(app, window, cx).is_some()) {
                copy(app, window, cx);
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        },
    ));
    root
}

pub(super) fn colors(
    background: Option<Rgba>,
    settings: &crate::features::preferences::Preferences,
    cx: &App,
) -> (Rgba, Option<Rgba>) {
    let base = background.unwrap_or(rgb(crate::components::style::CONTENT));
    let accent = rgb(
        crate::desktop::appearance::system_accent(settings.use_system_accent, cx)
            .unwrap_or(crate::components::style::TAB_INDICATOR),
    );
    let alpha = 0.30;
    let tinted = Rgba {
        r: base.r * (1.0 - alpha) + accent.r * alpha,
        g: base.g * (1.0 - alpha) + accent.g * alpha,
        b: base.b * (1.0 - alpha) + accent.b * alpha,
        a: 1.0,
    };
    let foreground =
        crate::components::contrast::selection_text([tinted.r, tinted.g, tinted.b].map(f64::from));
    (rgb(foreground), Some(tinted))
}
