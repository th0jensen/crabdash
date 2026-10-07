//! Native chrome controls share actions and geometry with GPUI fallbacks.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;

use crate::{
    app::Crabdash,
    components::{
        common::{control_tooltip, lucide_icon},
        style,
    },
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Control {
    Refresh,
    Terminal,
}
impl Control {
    fn id(self) -> &'static str {
        match self {
            Self::Refresh => "refresh-button",
            Self::Terminal => "toggle-quake-terminal",
        }
    }
    fn tooltip(self) -> &'static str {
        match self {
            Self::Refresh => crate::desktop::menus::shortcut("Refresh · ⌘R", "Refresh · Ctrl+R"),
            Self::Terminal => {
                crate::desktop::menus::shortcut("Toggle terminal · ⌘J", "Toggle terminal · Ctrl+J")
            }
        }
    }
    #[cfg(target_os = "macos")]
    fn action(self) -> Box<dyn Action> {
        match self {
            Self::Refresh => Box::new(crate::RefreshServices),
            Self::Terminal => Box::new(crate::ToggleTerminal),
        }
    }
}

pub(crate) fn render(
    control: Control,
    app: &Crabdash,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> AnyElement {
    platform::render(control, app, window, cx)
}

fn fallback(control: Control, app: &Crabdash, cx: &mut Context<Crabdash>) -> Stateful<Div> {
    let selected = control == Control::Terminal && app.quake_terminal_open;
    div()
        .id(control.id())
        .tooltip(move |_, cx| control_tooltip(control.tooltip(), cx))
        .size(rems(style::CHROME_CONTROL / 16.0))
        .when(control == Control::Terminal, |this| {
            this.flex_none().text_size(rems(style::TEXT / 16.0))
        })
        .rounded(px(style::RADIUS))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(if selected {
            style::TEXT_SELECTED
        } else {
            style::TEXT_MUTED
        }))
        .when(selected, |this| this.bg(rgb(style::CONTROL_SELECTED_BG)))
        .cursor_pointer()
        .hover(|this| {
            this.bg(rgb(style::SURFACE_HOVER))
                .text_color(rgb(style::TEXT_SELECTED))
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(lucide_icon(
            match control {
                Control::Refresh => Icon::RefreshCw,
                Control::Terminal => Icon::Terminal,
            },
            style::ICON,
        ))
        .on_click(cx.listener(move |this, _, window, cx| match control {
            Control::Refresh => {
                this.refresh_services(cx);
                cx.notify();
            }
            Control::Terminal => this.toggle_quake_terminal(window, cx),
        }))
}
