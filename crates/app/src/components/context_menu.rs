use crate::components::common::lucide_icon;
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use std::rc::Rc;

#[derive(Clone)]
struct ContextMenuEntry {
    label: SharedString,
    icon: Icon,
    color: Option<Rgba>,
    destructive: bool,
    handler: Rc<dyn Fn(&mut Window, &mut App)>,
}

pub struct ContextMenu {
    focus_handle: FocusHandle,
    entries: Vec<ContextMenuEntry>,
}

impl ContextMenu {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            entries: Vec::new(),
        }
    }

    pub fn build(
        window: &mut Window,
        cx: &mut App,
        builder: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Entity<Self> {
        cx.new(|cx| builder(Self::new(cx), window, cx))
    }

    pub fn entry(
        mut self,
        label: impl Into<SharedString>,
        icon: Icon,
        color: Option<Rgba>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.entries.push(ContextMenuEntry {
            label: label.into(),
            icon,
            color,
            destructive: false,
            handler: Rc::new(handler),
        });
        self
    }

    pub fn destructive_entry(
        mut self,
        label: impl Into<SharedString>,
        icon: Icon,
        color: Option<Rgba>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.entries.push(ContextMenuEntry {
            label: label.into(),
            icon,
            color,
            destructive: true,
            handler: Rc::new(handler),
        });
        self
    }
}

impl EventEmitter<DismissEvent> for ContextMenu {}

impl Focusable for ContextMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ContextMenu {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.entries.clone();
        div()
            .track_focus(&self.focus_handle(cx))
            .on_mouse_down_out(cx.listener(|_, _: &MouseDownEvent, _, cx| {
                cx.emit(DismissEvent);
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .w(px(180.0))
            .p(px(6.0))
            .bg(rgb(0x1C1C1E))
            .border_1()
            .border_color(rgb(0x2F2F31))
            .rounded(px(10.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .children(entries.into_iter().enumerate().map(|(index, entry)| {
                let handler = entry.handler.clone();
                let color = entry.color.unwrap_or(if entry.destructive {
                    rgb(0xFF453A)
                } else {
                    rgb(0xFFFFFF)
                });
                div()
                    .id(SharedString::from(format!("context-menu-item-{index}")))
                    .h(px(30.0))
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x2A2A2C)))
                    .text_sm()
                    .text_color(color)
                    .child(lucide_icon(entry.icon, 12.0))
                    .child(entry.label)
                    .on_click(cx.listener(move |_, _, window, cx| {
                        (handler)(window, cx);
                        cx.emit(DismissEvent);
                    }))
                    .into_any_element()
            }))
    }
}
