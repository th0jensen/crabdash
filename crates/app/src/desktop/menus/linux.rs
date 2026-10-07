//! Linux uses GPUI's registered application menus inside the window.
use crate::app::Crabdash;
use crate::components::style;
use crate::desktop::window::platform_title_bar_height;
use gpui::{prelude::*, *};

// Menus share the title bar with the window controls instead of taking a row
// from the dashboard. Keep them visible while navigating an open menu.
pub fn visible(app: &Crabdash, window: &Window) -> bool {
    (window.is_window_active() && window.modifiers().alt) || app.open_menu.is_some()
}

fn menu_width(index: usize) -> f32 {
    [80.0, 40.0, 40.0, 44.0, 64.0, 44.0][index.min(5)]
}

/// Show registered shortcuts even if their editing context is not focused.
/// Availability controls the menu item's enabled state, not discoverability.
fn action_shortcuts(action: &dyn Action, window: &Window, cx: &App) -> String {
    let bindings = if let Some(binding) = window.highest_precedence_binding_for_action(action) {
        vec![binding]
    } else {
        cx.key_bindings()
            .borrow()
            .bindings_for_action(action)
            .cloned()
            .collect()
    };
    let mut labels = Vec::new();
    for binding in bindings {
        let label = binding
            .keystrokes()
            .iter()
            .map(|stroke| {
                let mut keys = Vec::new();
                let modifiers = stroke.modifiers();
                if modifiers.control {
                    keys.push("Ctrl".to_string());
                }
                if modifiers.alt {
                    keys.push("Alt".to_string());
                }
                if modifiers.shift {
                    keys.push("Shift".to_string());
                }
                if modifiers.platform {
                    keys.push("Super".to_string());
                }
                keys.push(stroke.key().to_uppercase());
                keys.join("+")
            })
            .collect::<Vec<_>>()
            .join(" ");
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    labels.join(" / ")
}

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let menus = cx.get_menus().unwrap_or_default();
    div()
        .flex_none()
        .flex()
        .items_center()
        .children(menus.into_iter().enumerate().map(|(index, menu)| {
            div()
                .id(SharedString::from(format!("menu-{index}")))
                .w(gpui::rems(menu_width(index) / 16.0))
                .h(gpui::rems(style::CONTROL / 16.0))
                .flex_none()
                .rounded(px(3.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(gpui::rems(style::TEXT / 16.0))
                .text_color(rgb(0xC8C8C8))
                .cursor_pointer()
                .when(app.open_menu == Some(index), |this| this.bg(rgb(0x343434)))
                .hover(|this| this.bg(rgb(0x2B2B2B)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_move(cx.listener(move |this, _, _, cx| {
                    if this.open_menu.is_some() && this.open_menu != Some(index) {
                        this.open_menu = Some(index);
                        this.menu_item = 0;
                        cx.notify();
                    }
                }))
                .child(StyledText::new(menu.name).with_highlights([(
                    0..1,
                    HighlightStyle {
                        underline: Some(UnderlineStyle {
                            thickness: px(1.0),
                            color: None,
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                )]))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_menu = if this.open_menu == Some(index) {
                        None
                    } else {
                        Some(index)
                    };
                    this.menu_item = 0;
                    // Do not steal the text field's focus: Edit menu actions target it.
                    if window.focused(cx).is_none() {
                        window.focus(&this.focus_handle);
                    }
                    cx.notify();
                }))
        }))
}

pub fn popup(app: &Crabdash, window: &mut Window, cx: &mut Context<Crabdash>) -> AnyElement {
    let menus = cx.get_menus().unwrap_or_default();
    let index = app.open_menu.unwrap_or(0);
    let Some(menu) = menus.get(index) else {
        return div().into_any_element();
    };
    // Title-bar padding + sidebar toggle + the gap before the menu headings.
    let left = 18.0
        + (28.0 + (0..index).map(menu_width).sum::<f32>()) * f32::from(window.rem_size()) / 16.0;
    let items = menu.items.clone();
    div()
        .id("application-menu-popup")
        .absolute()
        .left(px(left))
        .top(platform_title_bar_height(window))
        .w(gpui::rems(304.0 / 16.0))
        .p(px(5.0))
        .rounded(px(6.0))
        .shadow_lg()
        .bg(rgb(0x242424))
        .border_1()
        .border_color(rgb(0x444444))
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
            this.open_menu = None;
            cx.notify();
        }))
        .children(items.into_iter().enumerate().map(|(item_index, item)| {
            match item {
                OwnedMenuItem::Separator => div()
                    .h(px(1.0))
                    .my(px(5.0))
                    .mx(px(8.0))
                    .bg(rgb(0x3A3A3A))
                    .into_any_element(),
                OwnedMenuItem::Action { name, action, .. } => {
                    let enabled = window.is_action_available(action.as_ref(), cx)
                        || cx.is_action_available(action.as_ref());
                    let shortcut = action_shortcuts(action.as_ref(), window, cx);
                    div()
                        .id(SharedString::from(format!(
                            "menu-entry-{index}-{item_index}"
                        )))
                        .h(gpui::rems(style::CONTROL / 16.0))
                        .px(px(10.0))
                        .rounded(px(3.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .text_size(gpui::rems(style::TEXT / 16.0))
                        .text_color(if enabled {
                            rgb(0xE0E0E0)
                        } else {
                            rgb(0x777777)
                        })
                        .when(enabled, |this| {
                            this.cursor_pointer().hover(|this| this.bg(rgb(0x3A3A3A)))
                        })
                        .when(app.menu_item == item_index && enabled, |this| {
                            this.bg(rgb(0x3A3A3A))
                        })
                        .child(name)
                        .child(
                            div()
                                .text_size(gpui::rems(style::META / 16.0))
                                .text_color(rgb(0x909090))
                                .child(shortcut),
                        )
                        .on_mouse_move(cx.listener(move |this, _, _, cx| {
                            this.menu_item = item_index;
                            cx.notify();
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if enabled {
                                this.open_menu = None;
                                window.dispatch_action(action.boxed_clone(), cx);
                                cx.notify();
                            }
                        }))
                        .into_any_element()
                }
                _ => div().into_any_element(),
            }
        }))
        .into_any_element()
}

pub fn handle_key(
    app: &mut Crabdash,
    keystroke: &Keystroke,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) {
    let menus = cx.get_menus().unwrap_or_default();
    if menus.is_empty() {
        return;
    }
    if keystroke.modifiers.alt {
        let index = match keystroke.key.as_str() {
            "c" => Some(0),
            "f" => Some(1),
            "e" => Some(2),
            "v" => Some(3),
            "w" => Some(4),
            "h" => Some(5),
            _ => None,
        };
        if let Some(index) = index.filter(|index| *index < menus.len()) {
            app.open_menu = Some(index);
            app.menu_item = 0;
            cx.stop_propagation();
            cx.notify();
            return;
        }
    }
    let Some(index) = app.open_menu.filter(|index| *index < menus.len()) else {
        return;
    };
    let selectable: Vec<_> = menus[index]
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            OwnedMenuItem::Action { action, .. }
                if window.is_action_available(action.as_ref(), cx)
                    || cx.is_action_available(action.as_ref()) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect();
    match keystroke.key.as_str() {
        "escape" => app.open_menu = None,
        "left" => {
            app.open_menu = Some((index + menus.len() - 1) % menus.len());
            app.menu_item = 0;
        }
        "right" => {
            app.open_menu = Some((index + 1) % menus.len());
            app.menu_item = 0;
        }
        "up" | "down" if !selectable.is_empty() => {
            let position = selectable.iter().position(|index| *index == app.menu_item);
            let next = match (position, keystroke.key.as_str()) {
                (Some(position), "up") => (position + selectable.len() - 1) % selectable.len(),
                (Some(position), _) => (position + 1) % selectable.len(),
                (None, "up") => selectable.len() - 1,
                (None, _) => 0,
            };
            app.menu_item = selectable[next];
        }
        "enter" | "return" => {
            if let Some(OwnedMenuItem::Action { action, .. }) =
                menus[index].items.get(app.menu_item)
            {
                if window.is_action_available(action.as_ref(), cx)
                    || cx.is_action_available(action.as_ref())
                {
                    window.dispatch_action(action.boxed_clone(), cx);
                    app.open_menu = None;
                }
            }
        }
        _ => return,
    }
    cx.stop_propagation();
    cx.notify();
}

pub(super) fn append_app_items(_: &mut Vec<MenuItem>) {}
pub(super) fn register_actions(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("f10", crate::ToggleAppMenu, None),
        KeyBinding::new("ctrl-shift-m", crate::ZoomWindow, None),
    ]);
}
pub(super) fn shortcut(_macos: &'static str, linux: &'static str) -> &'static str {
    linux
}
pub(super) fn intercept(cx: &mut Context<Crabdash>) -> Option<Subscription> {
    let listener = cx.listener(|this, event: &KeystrokeEvent, window, cx| {
        if this.focus_handle.contains_focused(window, cx) {
            handle_key(this, &event.keystroke, window, cx);
        }
    });
    Some(cx.intercept_keystrokes(listener))
}
