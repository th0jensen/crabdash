use super::{
    TerminalCursor, TerminalSpan, docking, geometry, grid, header, height, panes, selection, split,
};
use gpui::prelude::*;
use gpui::*;
use libghostty_vt::terminal::ScrollViewport;
use lucide_icons::Icon;

use crate::{
    app::Crabdash,
    components::{common::surface_button, style},
};

pub(crate) fn cell_metrics(
    settings: &crate::features::preferences::Preferences,
    cx: &App,
) -> (f32, f32) {
    let font_id = cx
        .text_system()
        .resolve_font(&font(settings.terminal_font.clone()));
    let width = cx
        .text_system()
        .advance(font_id, px(settings.terminal_font_size), 'M')
        .map(|advance| f32::from(advance.width))
        .unwrap_or(settings.terminal_font_size * 0.6)
        .max(1.0);
    (
        width,
        (settings.terminal_font_size * settings.terminal_line_height).ceil(),
    )
}

pub(crate) fn quake_height_for_rows(rows: u16, cell_height: f32, header_height: f32) -> Pixels {
    px(geometry::height_for_rows(rows, cell_height, header_height))
}

/// Render a Ghostty terminal frame as styled text rows.
pub fn render_view(logs: &[Vec<TerminalSpan>], cx: &App) -> Div {
    render_view_with_cursor(logs, None, rgb(style::TEXT_PRIMARY), cx)
}

fn render_view_with_cursor(
    logs: &[Vec<TerminalSpan>],
    cursor: Option<TerminalCursor>,
    default_fg: Rgba,
    cx: &App,
) -> Div {
    let settings = crate::features::preferences::current(cx);
    let (char_width, line_height) = cell_metrics(&settings, cx);
    grid::render(
        logs,
        cursor,
        default_fg,
        settings,
        char_width,
        line_height,
        cx,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuakeTerminalStatus {
    Connecting,
    Connected,
    Exited,
    Failed,
}

fn content(
    app: &Crabdash,
    session: uuid::Uuid,
    quake: &super::QuakeTerminal,
    window: &Window,
    cx: &mut Context<Crabdash>,
) -> Div {
    let machine = app.selected_machine().uuid;
    div()
        .size_full()
        .min_w_0()
        .min_h_0()
        .relative()
        .px(px(geometry::CONTENT_SIDE))
        .pt(px(geometry::CONTENT_TOP))
        .pb(px(geometry::CONTENT_BOTTOM))
        .overflow_hidden()
        .cursor_text()
        .on_scroll_wheel(cx.listener(move |app, event: &ScrollWheelEvent, _, cx| {
            let (_, line_height) = cell_metrics(&app.preferences, cx);
            if app.selected_machine().uuid == machine
                && let Some(quake) = app
                    .quake_terminals
                    .get_mut(&machine)
                    .and_then(|drawer| drawer.sessions.get_mut(&session))
            {
                quake.terminal.scroll_wheel(event.delta, line_height);
                cx.notify();
                cx.stop_propagation();
            }
        }))
        .when(quake.terminal.rendered.is_empty(), |this| {
            this.child(
                div()
                    .font_family(app.preferences.terminal_font.clone())
                    .text_size(rems(style::META / 16.0))
                    .text_color(rgb(style::TEXT_MUTED))
                    .child("Starting terminal session…"),
            )
        })
        .when(!quake.terminal.rendered.is_empty(), |this| {
            this.child(render_view_with_cursor(
                &quake.terminal.rendered,
                quake.terminal.cursor,
                rgba(0xAEAEB2FF),
                cx,
            ))
        })
        .child(quake.input.clone())
        .child(selection::layer(app, machine, session, window, cx))
        .when(!quake.terminal.following_output(), |this| {
            this.child(
                surface_button(
                    SharedString::from(format!("terminal-follow-output-{session}")),
                    Some(Icon::ArrowDown),
                    Some("Latest output"),
                )
                .absolute()
                .right(px(12.0))
                .bottom(px(8.0))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, _, window, cx| {
                        // Occlusion protects the selection source and bypasses pane capture.
                        app.select_terminal_session(machine, session, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_click(cx.listener(move |app, _, window, cx| {
                    if app.selected_machine().uuid == machine
                        && let Some(quake) = app
                            .quake_terminals
                            .get_mut(&machine)
                            .and_then(|drawer| drawer.sessions.get_mut(&session))
                    {
                        quake.terminal.scroll(ScrollViewport::Bottom);
                        window.focus(&quake.input.focus_handle(cx));
                        cx.notify();
                    }
                })),
            )
        })
}

fn render_node(
    app: &Crabdash,
    node: &crate::layout::Node<uuid::Uuid>,
    width: Pixels,
    height: Pixels,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    match node {
        crate::layout::Node::Pane { id, tabs, active } => {
            let pane = *id;
            let machine = app.selected_machine().uuid;
            let active = *active;
            let Some(quake) = app
                .quake_terminals
                .get(&machine)
                .and_then(|drawer| drawer.sessions.get(&active))
            else {
                return div().id("missing-terminal-session");
            };
            let panel = content(app, active, quake, window, cx);
            let editing = quake.rename.is_some();
            div()
                .id(SharedString::from(format!(
                    "terminal-pane-{machine}-{pane}"
                )))
                .w(width)
                .h(height)
                .flex_none()
                .min_w_0()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .flex()
                .flex_col()
                .when(!editing, |this| {
                    this.capture_any_mouse_down(cx.listener(move |app, _, window, cx| {
                        app.select_terminal_session(machine, active, window, cx)
                    }))
                })
                .child(header::tabs(app, pane, tabs, active, width, window, cx))
                .child(docking::body(app, pane, panel, cx))
        }
        crate::layout::Node::Split { .. } => {
            split::render(app, node, width, height, window, cx, render_node)
        }
    }
}

pub(crate) fn render_quake(
    app: &Crabdash,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> impl IntoElement {
    let machine = app.selected_machine().uuid;
    let Some(drawer) = app.quake_terminals.get(&machine) else {
        return div();
    };
    let Some(_) = drawer.active() else {
        return div();
    };
    let focused = drawer
        .sessions
        .values()
        .any(|session| session.input.focus_handle(cx).is_focused(window));
    let (width, _) = panes::viewport(window);
    let height = (app.quake_height - px(geometry::PANEL_BORDER)).max(px(0.0));
    let body = render_node(
        app,
        &drawer.model.layout.root,
        px(width),
        height,
        window,
        cx,
    );
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(app.quake_height)
        .bg(rgb(style::CONTENT))
        .border_t_1()
        .border_color(rgb(if focused {
            style::FOCUS_BORDER
        } else {
            style::BORDER
        }))
        .shadow_lg()
        .flex()
        .flex_col()
        .occlude()
        .on_drag_move::<height::ResizeDrag>(cx.listener(
            |app, event: &DragMoveEvent<height::ResizeDrag>, window, cx| {
                let token = event.drag(cx).token;
                if !token.valid(app, cx.entity_id()) {
                    return;
                }
                cx.set_active_drag_cursor_style(CursorStyle::ResizeUpDown, window);
                app.update_quake_resize(token, event.event.position.y, window, cx);
            },
        ))
        .child(body)
        .child(height::resize_header(
            div()
                .id("terminal-drawer-upper-edge")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(3.0))
                // This invisible input strip must not occlude native toolbar material.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            app,
            cx,
        ))
}
