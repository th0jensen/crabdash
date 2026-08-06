use std::ops::Range;

use gpui::{
    App, Bounds, Context, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, LayoutId, MouseButton, MouseDownEvent, Pixels, Point,
    Style, UTF16Selection, Window, actions, div, prelude::*, relative,
};
use machines::terminal::TerminalController;

use super::text_field::FieldPaste;

actions!(
    crabdash_terminal_input,
    [
        TerminalBackspace,
        TerminalDelete,
        TerminalEnter,
        TerminalEscape,
        TerminalTab,
        TerminalShiftTab,
        TerminalUp,
        TerminalDown,
        TerminalLeft,
        TerminalRight,
        TerminalHome,
        TerminalEnd,
        TerminalPageUp,
        TerminalPageDown,
        TerminalInterrupt,
        TerminalEof,
        TerminalSuspend,
        TerminalClear,
    ]
);

pub struct TerminalInput {
    focus_handle: FocusHandle,
    controller: Option<TerminalController>,
    marked_text: Option<String>,
}

impl TerminalInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            controller: None,
            marked_text: None,
        }
    }

    pub fn set_controller(
        &mut self,
        controller: Option<TerminalController>,
        cx: &mut Context<Self>,
    ) {
        self.controller = controller;
        cx.notify();
    }

    fn write(&self, bytes: impl Into<Vec<u8>>) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        if let Err(error) = controller.write(bytes) {
            tracing::warn!(%error, "Failed to write terminal input");
        }
    }

    fn backspace(&mut self, _: &TerminalBackspace, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x7f".to_vec());
    }

    fn delete(&mut self, _: &TerminalDelete, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[3~".to_vec());
    }

    fn enter(&mut self, _: &TerminalEnter, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\r".to_vec());
    }

    fn escape(&mut self, _: &TerminalEscape, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b".to_vec());
    }

    fn tab(&mut self, _: &TerminalTab, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\t".to_vec());
    }

    fn shift_tab(&mut self, _: &TerminalShiftTab, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[Z".to_vec());
    }

    fn up(&mut self, _: &TerminalUp, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[A".to_vec());
    }

    fn down(&mut self, _: &TerminalDown, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[B".to_vec());
    }

    fn left(&mut self, _: &TerminalLeft, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[D".to_vec());
    }

    fn right(&mut self, _: &TerminalRight, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[C".to_vec());
    }

    fn home(&mut self, _: &TerminalHome, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[H".to_vec());
    }

    fn end(&mut self, _: &TerminalEnd, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[F".to_vec());
    }

    fn page_up(&mut self, _: &TerminalPageUp, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[5~".to_vec());
    }

    fn page_down(&mut self, _: &TerminalPageDown, _: &mut Window, _: &mut Context<Self>) {
        self.write(b"\x1b[6~".to_vec());
    }

    fn interrupt(&mut self, _: &TerminalInterrupt, _: &mut Window, _: &mut Context<Self>) {
        self.write(vec![0x03]);
    }

    fn eof(&mut self, _: &TerminalEof, _: &mut Window, _: &mut Context<Self>) {
        self.write(vec![0x04]);
    }

    fn suspend(&mut self, _: &TerminalSuspend, _: &mut Window, _: &mut Context<Self>) {
        self.write(vec![0x1a]);
    }

    fn clear(&mut self, _: &TerminalClear, _: &mut Window, _: &mut Context<Self>) {
        self.write(vec![0x0c]);
    }

    fn paste(&mut self, _: &FieldPaste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.write(text.into_bytes());
        }
    }

    fn focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle(cx));
        cx.stop_propagation();
    }
}

impl EntityInputHandler for TerminalInput {
    fn text_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        actual_range.replace(0..0);
        Some(String::new())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(text) = self.marked_text.take() {
            self.write(text.into_bytes());
        }
    }

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.marked_text = None;
        if new_text == "\n" {
            self.write(b"\r".to_vec());
        } else {
            self.write(new_text.as_bytes().to_vec());
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.marked_text = Some(new_text.to_string());
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}

struct TerminalInputElement {
    input: Entity<TerminalInput>,
}

impl IntoElement for TerminalInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalInputElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
    }
}

impl Render for TerminalInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("terminal-input-capture")
            .key_context("CrabdashTerminalInput")
            .absolute()
            .inset_0()
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::shift_tab))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::interrupt))
            .on_action(cx.listener(Self::eof))
            .on_action(cx.listener(Self::suspend))
            .on_action(cx.listener(Self::clear))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::focus))
            .child(TerminalInputElement { input: cx.entity() })
    }
}

impl Focusable for TerminalInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
