use crate::components::style;
use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div,
    fill, point, prelude::*, px, relative, rgb, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    crabdash_text_field,
    [
        FieldBackspace,
        FieldDelete,
        FieldLeft,
        FieldRight,
        FieldSelectLeft,
        FieldSelectRight,
        FieldSelectAll,
        FieldHome,
        FieldEnd,
        FieldPaste,
        FieldCut,
        FieldCopy,
        FieldTab,
        FieldTabPrev,
    ]
);

pub struct TextField {
    focus_handle: FocusHandle,
    label: SharedString,
    placeholder: SharedString,
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    horizontal_scroll: Pixels,
    is_selecting: bool,
    compact: bool,
    native_search: bool,
    native_model_revision: u64,
}

impl TextField {
    pub fn new(
        label: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        tab_index: isize,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_index(tab_index).tab_stop(true),
            label: label.into(),
            placeholder: placeholder.into(),
            content: SharedString::new(""),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            horizontal_scroll: px(0.0),
            is_selecting: false,
            compact: false,
            native_search: false,
            native_model_revision: 0,
        }
    }

    pub(crate) fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub(crate) fn native_search(mut self) -> Self {
        self.native_search = true;
        self
    }

    pub fn text(&self) -> String {
        self.content.to_string()
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_text("", cx);
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.native_model_revision = self.native_model_revision.wrapping_add(1);
        self.set_content(text, cx);
    }

    pub(crate) fn select_all_text(&mut self, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn native_model_revision(&self) -> u64 {
        self.native_model_revision
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn set_native_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.set_content(text, cx);
    }

    fn set_content(&mut self, text: &str, cx: &mut Context<Self>) {
        self.content = single_line(text).into();
        let end = self.content.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    fn left(&mut self, _: &FieldLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &FieldRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &FieldSelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &FieldSelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &FieldSelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.select_all_text(cx);
    }

    fn home(&mut self, _: &FieldHome, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &FieldEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &FieldBackspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }

        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &FieldDelete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }

        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn paste(&mut self, _: &FieldPaste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
        }
    }

    fn copy(&mut self, _: &FieldCopy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &FieldCut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };

        line.closest_index_for_x(line_x(position.x, bounds.left(), self.horizontal_scroll))
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }

        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }

        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        utf16_offset(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }

            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }
}

fn single_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

fn utf16_offset(text: &str, offset: usize) -> usize {
    let mut bytes = 0;
    let mut units = 0;
    for ch in text.chars() {
        if units >= offset {
            break;
        }
        units += ch.len_utf16();
        bytes += ch.len_utf8();
    }
    bytes
}

fn composed_selection(text: &str, selected: &Range<usize>, insertion: usize) -> Range<usize> {
    let start = utf16_offset(text, selected.start);
    let end = utf16_offset(text, selected.end).max(start);
    insertion + start..insertion + end
}

fn line_x(screen: Pixels, left: Pixels, scroll: Pixels) -> Pixels {
    screen - left + scroll
}
fn screen_x(line: Pixels, left: Pixels, scroll: Pixels) -> Pixels {
    left + line - scroll
}

/// Reserve the two-pixel caret inside the viewport, and discard obsolete scroll
/// after edits or resizing. Selection follows its moving endpoint in either direction.
fn caret_scroll(previous: Pixels, caret: Pixels, text_width: Pixels, viewport: Pixels) -> Pixels {
    let visible = (viewport - px(2.0)).max(px(0.0));
    let max_scroll = (text_width - visible).max(px(0.0));
    let scroll = previous.max(px(0.0)).min(max_scroll);
    if caret < scroll {
        caret.max(px(0.0)).min(max_scroll)
    } else if caret > scroll + visible {
        (caret - visible).min(max_scroll)
    } else {
        scroll
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let new_text = single_line(new_text);
        self.content =
            (self.content[0..range.start].to_owned() + &new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        self.selection_reversed = false;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let new_text = single_line(new_text);
        self.content =
            (self.content[0..range.start].to_owned() + &new_text + &self.content[range.end..])
                .into();
        self.marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|selected| composed_selection(&new_text, selected, range.start))
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.selection_reversed = false;
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);

        Some(Bounds::from_corners(
            point(
                screen_x(
                    line.x_for_index(range.start),
                    bounds.left(),
                    self.horizontal_scroll,
                )
                .max(bounds.left())
                .min(bounds.right()),
                bounds.top(),
            ),
            point(
                screen_x(
                    line.x_for_index(range.end),
                    bounds.left(),
                    self.horizontal_scroll,
                )
                .max(bounds.left())
                .min(bounds.right()),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let line = self.last_layout.as_ref()?;
        let utf8_index = if self.content.is_empty() {
            0
        } else {
            line.closest_index_for_x(line_point.x + self.horizontal_scroll)
        };

        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextFieldElement {
    input: Entity<TextField>,
}

struct PrepaintState {
    line: ShapedLine,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    horizontal_scroll: Pixels,
}

impl IntoElement for TextFieldElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextFieldElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

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
        style.size.height = gpui::rems(18.0 / 16.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let previous_scroll = input.horizontal_scroll;

        let display_text = if content.is_empty() {
            input.placeholder.clone()
        } else {
            content
        };
        let text_color = if input.content.is_empty() {
            rgb(0x6C6C70)
        } else {
            rgb(0xE5E5EA)
        };

        let run = TextRun {
            len: display_text.len(),
            font: window.text_style().font(),
            color: text_color.into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = window.text_style().font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        let cursor_x = line.x_for_index(cursor);
        let horizontal_scroll =
            caret_scroll(previous_scroll, cursor_x, line.width, bounds.size.width);
        let origin_x = bounds.left() - horizontal_scroll;
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(origin_x + cursor_x, bounds.top() + px(1.0)),
                        size(px(2.0), bounds.bottom() - bounds.top() - px(2.0)),
                    ),
                    rgb(0xD4D4D4),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            origin_x + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            origin_x + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0xFFFFFF24),
                )),
                None,
            )
        };

        PrepaintState {
            line,
            cursor,
            selection,
            horizontal_scroll,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        let line = &prepaint.line;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection);
            }
            let origin = point(bounds.left() - prepaint.horizontal_scroll, bounds.top());
            if let Err(error) = line.paint(origin, window.line_height(), window, cx) {
                tracing::error!(%error, "Failed to paint text field text");
            }
            if focus_handle.is_focused(window)
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line.clone());
            input.last_bounds = Some(bounds);
            input.horizontal_scroll = prepaint.horizontal_scroll;
        });
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle(cx).is_focused(window);

        let field = div()
            .id("text-field-input")
            .key_context("CrabdashTextField")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .h(gpui::rems(
                if self.compact { style::CONTROL } else { 36.0 } / 16.0,
            ))
            .px(px(10.0))
            .flex()
            .items_center()
            .bg(rgb(style::SURFACE))
            .border_1()
            .border_color(if focused {
                rgb(style::FOCUS_BORDER)
            } else {
                rgb(style::BORDER)
            })
            .rounded(px(if self.compact {
                style::CARD_RADIUS
            } else {
                style::RADIUS
            }))
            .line_height(gpui::rems(18.0 / 16.0))
            .text_size(gpui::rems(style::TEXT / 16.0))
            .child(TextFieldElement { input: cx.entity() });
        let field = if self.native_search {
            crate::desktop::controls::search(field, cx.entity(), self.placeholder.clone())
        } else {
            field.into_any_element()
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .when(!self.label.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(gpui::rems(style::META / 16.0))
                        .text_color(rgb(0xAEAEB2))
                        .child(self.label.clone()),
                )
            })
            .child(field)
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{caret_scroll, composed_selection, line_x, screen_x, single_line};
    use gpui::px;
    use std::prelude::v1::test;

    #[test]
    fn caret_scroll_reveals_pasted_end_and_home_then_clamps_after_resize_or_delete() {
        let end = caret_scroll(px(0.0), px(640.0), px(640.0), px(120.0));
        assert_eq!(end, px(522.0));
        assert_eq!(screen_x(px(640.0), px(10.0), end), px(128.0));
        assert_eq!(caret_scroll(end, px(0.0), px(640.0), px(120.0)), px(0.0));
        assert_eq!(caret_scroll(end, px(40.0), px(40.0), px(120.0)), px(0.0));
        assert_eq!(caret_scroll(end, px(640.0), px(640.0), px(700.0)), px(0.0));
        assert_eq!(caret_scroll(end, px(640.0), px(640.0), px(0.0)), px(640.0));
    }

    #[test]
    fn scrolled_mouse_and_ime_coordinates_are_inverse_and_selection_endpoint_visible() {
        let left = px(50.0);
        let scroll = caret_scroll(px(0.0), px(300.0), px(640.0), px(120.0));
        let mouse = px(90.0);
        assert_eq!(screen_x(line_x(mouse, left, scroll), left, scroll), mouse);
        assert_eq!(line_x(mouse, left, scroll), px(222.0));
        let reversed = caret_scroll(scroll, px(100.0), px(640.0), px(120.0));
        assert_eq!(screen_x(px(100.0), left, reversed), left);
    }

    #[test]
    fn ime_selection_is_relative_to_inserted_unicode_not_replaced_range_end() {
        assert_eq!(composed_selection("é😀z", &(1..3), 5), 7..11);
        assert_eq!(composed_selection("é😀z", &(99..99), 5), 12..12);
        assert_eq!(single_line("a\r\nb\nc"), "a  b c");
    }
}
