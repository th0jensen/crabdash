//! Native controls share actions and geometry with GPUI fallbacks.
#[cfg(any(target_os = "macos", test))]
mod events;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod modal;
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
        common::{chrome_icon_button, control_tooltip},
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
    chrome_icon_button(
        control.id(),
        match control {
            Control::Refresh => Icon::RefreshCw,
            Control::Terminal => Icon::Terminal,
        },
    )
    .tooltip(move |_, cx| control_tooltip(control.tooltip(), cx))
    .when(control == Control::Terminal, |this| {
        this.text_size(rems(style::TEXT / 16.0))
    })
    .when(selected, |this| {
        this.text_color(rgb(style::TEXT_SELECTED))
            .bg(rgb(style::CONTROL_SELECTED_BG))
    })
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .on_click(cx.listener(move |this, _, window, cx| match control {
        Control::Refresh => {
            this.refresh_services(cx);
            cx.notify();
        }
        Control::Terminal => this.toggle_quake_terminal(window, cx),
    }))
}

#[cfg(target_os = "macos")]
pub(crate) use macos::SurfaceControl;
#[cfg(not(target_os = "macos"))]
pub(crate) type SurfaceControl = Stateful<Div>;
#[cfg(target_os = "macos")]
pub(crate) type StatusControl = SurfaceControl;
#[cfg(not(target_os = "macos"))]
pub(crate) type StatusControl = Div;

pub(crate) fn button(
    fallback: Stateful<Div>,
    label: impl Into<SharedString>,
    icon: Option<Icon>,
) -> SurfaceControl {
    selected_button_inner(fallback, label.into(), icon, None, false)
}
/// A stock native row action cluster, with the original children retained as
/// the action and layout authority on every platform and appearance setting.
pub(crate) fn action_group(fallback: Div, children: Vec<SurfaceControl>) -> AnyElement {
    #[cfg(target_os = "macos")]
    {
        macos::Actions::new(fallback.id("native-action-group").children(children))
            .into_any_element()
    }
    #[cfg(not(target_os = "macos"))]
    {
        fallback.children(children).into_any_element()
    }
}

/// Window-style toolbar actions, kept separate from content-row controls.
pub(crate) fn toolbar_group(fallback: Div, children: Vec<SurfaceControl>) -> AnyElement {
    #[cfg(target_os = "macos")]
    {
        macos::Actions::toolbar(fallback.id("native-toolbar-group").children(children))
            .into_any_element()
    }
    #[cfg(not(target_os = "macos"))]
    {
        fallback.children(children).into_any_element()
    }
}

pub(crate) fn toolbar_icon_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: Icon,
) -> SurfaceControl {
    let control = icon_button(chrome_icon_button(id, icon), label, icon);
    #[cfg(target_os = "macos")]
    {
        control.set_role(macos::ButtonRole::Toolbar)
    }
    #[cfg(not(target_os = "macos"))]
    {
        control
    }
}
/// Match the row and heading allocation to AppKit's actual icon-button metrics.
pub(crate) fn action_group_width(
    fallback: f32,
    slots: &[&[Icon]],
    liquid_glass: bool,
    window: &Window,
    cx: &mut App,
) -> Pixels {
    let shared = px(fallback * f32::from(window.rem_size()) / 16.0);
    #[cfg(target_os = "macos")]
    {
        macos::action_group_width(shared, slots, liquid_glass, false, window, cx)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (slots, liquid_glass, cx);
        shared
    }
}

pub(crate) fn toolbar_group_width(
    fallback: f32,
    slots: &[&[Icon]],
    liquid_glass: bool,
    window: &Window,
    cx: &mut App,
) -> Pixels {
    #[cfg(target_os = "macos")]
    {
        macos::action_group_width(
            px(fallback * f32::from(window.rem_size()) / 16.0),
            slots,
            liquid_glass,
            true,
            window,
            cx,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        action_group_width(fallback, slots, liquid_glass, window, cx)
    }
}
pub(crate) fn selected_button(
    fallback: Stateful<Div>,
    label: impl Into<SharedString>,
    icon: Option<Icon>,
    selected: bool,
) -> SurfaceControl {
    selected_button_inner(fallback, label.into(), icon, Some(selected), false)
}
pub(crate) fn icon_button(
    fallback: Stateful<Div>,
    label: impl Into<SharedString>,
    icon: impl Into<Option<Icon>>,
) -> SurfaceControl {
    selected_button_inner(fallback, label.into(), icon.into(), None, true)
}
pub(crate) fn selected_icon_button(
    fallback: Stateful<Div>,
    label: impl Into<SharedString>,
    icon: impl Into<Option<Icon>>,
    selected: bool,
) -> SurfaceControl {
    selected_button_inner(fallback, label.into(), icon.into(), Some(selected), true)
}
fn selected_button_inner(
    fallback: Stateful<Div>,
    label: SharedString,
    icon: Option<Icon>,
    selected: Option<bool>,
    icon_only: bool,
) -> SurfaceControl {
    #[cfg(target_os = "macos")]
    {
        macos::SurfaceControl::new(
            fallback,
            macos::Spec::Button {
                label: if icon_only {
                    SharedString::default()
                } else {
                    label.clone()
                },
                accessibility: label,
                icon,
                selected,
                role: macos::ButtonRole::Normal,
            },
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (label, icon, selected, icon_only);
        fallback
    }
}
pub(crate) fn primary(control: SurfaceControl) -> SurfaceControl {
    #[cfg(target_os = "macos")]
    {
        control.set_role(macos::ButtonRole::Primary)
    }
    #[cfg(not(target_os = "macos"))]
    {
        control
    }
}
pub(crate) fn destructive(control: SurfaceControl) -> SurfaceControl {
    #[cfg(target_os = "macos")]
    {
        control.set_role(macos::ButtonRole::Destructive)
    }
    #[cfg(not(target_os = "macos"))]
    {
        control
    }
}
#[cfg(target_os = "macos")]
pub(crate) fn choices(
    control: SurfaceControl,
    input: Entity<crate::components::text_field::TextField>,
    values: Vec<String>,
    help: impl Into<SharedString>,
) -> SurfaceControl {
    control.set_choices(input, values, help.into())
}
pub(crate) fn enabled(control: SurfaceControl, enabled: bool) -> SurfaceControl {
    #[cfg(target_os = "macos")]
    {
        control.set_enabled(enabled)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = enabled;
        control
    }
}
pub(crate) fn selected(control: SurfaceControl, selected: bool) -> SurfaceControl {
    #[cfg(target_os = "macos")]
    {
        control.set_selected(selected)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = selected;
        control
    }
}
pub(crate) fn status(
    fallback: Div,
    label: impl Into<SharedString>,
    color: Rgba,
    semantic: bool,
) -> StatusControl {
    #[cfg(target_os = "macos")]
    {
        macos::SurfaceControl::new(
            fallback.id("native-status"),
            macos::Spec::Status {
                label: label.into(),
                color,
                semantic,
            },
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (label, color, semantic);
        fallback
    }
}
pub(crate) fn toggle(
    fallback: Stateful<Div>,
    label: impl Into<SharedString>,
    value: bool,
) -> SurfaceControl {
    #[cfg(target_os = "macos")]
    {
        macos::SurfaceControl::new(
            fallback,
            macos::Spec::Switch {
                label: label.into(),
                value,
            },
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (label, value);
        fallback
    }
}
pub(crate) fn search(
    fallback: Stateful<Div>,
    input: Entity<crate::components::text_field::TextField>,
    placeholder: SharedString,
) -> AnyElement {
    #[cfg(target_os = "macos")]
    {
        macos::SurfaceControl::new(fallback, macos::Spec::Search { input, placeholder })
            .into_any_element()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (input, placeholder);
        fallback.into_any_element()
    }
}

/// Retain native material outside the actual dialog while its backdrop blocks input.
pub(crate) fn modal(inner: Div, backdrop_opacity: f32) -> AnyElement {
    #[cfg(target_os = "macos")]
    {
        modal::Modal::new(inner, backdrop_opacity).into_any_element()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = backdrop_opacity;
        inner.into_any_element()
    }
}

/// A stock leading title that can shrink beside its disclosure.
pub(crate) fn leading_identity(
    fallback: Stateful<Div>,
    label: SharedString,
    selected: bool,
) -> SurfaceControl {
    let control = selected_button(fallback, label, None, selected);
    #[cfg(target_os = "macos")]
    {
        control.leading_identity()
    }
    #[cfg(not(target_os = "macos"))]
    {
        control
    }
}
pub(crate) fn stock_active(liquid_glass: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::active(liquid_glass)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = liquid_glass;
        false
    }
}
