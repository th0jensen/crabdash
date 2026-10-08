use crate::components::style;
use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use machines::machine::MachineKind;

pub type LucideIcon = Icon;

pub const LUCIDE_FONT_FAMILY: &str = "lucide";

struct ControlTooltip(SharedString);

impl Render for ControlTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        tooltip_text(self.0.clone())
    }
}

pub fn tooltip_text(label: impl Into<SharedString>) -> Div {
    div()
        .max_w(rems(360.0 / 16.0))
        .whitespace_normal()
        .px(px(8.0))
        .py(px(5.0))
        .rounded(px(style::RADIUS))
        .bg(rgb(0x242424))
        .border_1()
        .border_color(rgb(0x383838))
        .text_size(gpui::rems(style::META / 16.0))
        .text_color(rgb(0xD4D4D4))
        .child(label.into())
}

pub fn control_tooltip(label: impl Into<SharedString>, cx: &mut App) -> AnyView {
    let label = label.into();
    cx.new(|_| ControlTooltip(label)).into()
}

pub fn clipped_text(text: impl Into<SharedString>) -> Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        // GPUI 0.2 caches nowrap text before flex widths resolve. A one-line
        // clamp remeasures truncation against the final width.
        .whitespace_normal()
        .line_clamp(1)
        .child(text.into())
}

pub fn machine_icon(kind: MachineKind) -> LucideIcon {
    match kind {
        MachineKind::MacOS => Icon::Laptop,
        MachineKind::Linux => Icon::Server,
        MachineKind::Windows | MachineKind::Unknown => Icon::Monitor,
    }
}

pub fn lucide_icon(icon: LucideIcon, size: f32) -> Div {
    div()
        .flex_none()
        .font_family(LUCIDE_FONT_FAMILY)
        .text_size(gpui::rems(size / 16.0))
        .child(char::from(icon).to_string())
}

/// Compact, transparent window chrome shared by titlebar and terminal actions.
pub(crate) fn chrome_icon_button(id: impl Into<ElementId>, icon: Icon) -> Stateful<Div> {
    div()
        .id(id)
        .size(rems(style::CHROME_CONTROL / 16.0))
        .flex_none()
        .rounded(px(style::RADIUS))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(style::TEXT_MUTED))
        .cursor_pointer()
        .hover(|this| {
            this.bg(rgb(style::SURFACE_HOVER))
                .text_color(rgb(style::TEXT_SELECTED))
        })
        .child(lucide_icon(icon, style::ICON))
}

pub(crate) fn button<I, L>(
    id: impl Into<ElementId>,
    icon: Option<I>,
    label: Option<L>,
    primary: bool,
) -> crate::desktop::controls::SurfaceControl
where
    L: Into<SharedString>,
    I: Into<LucideIcon>,
{
    let has_label = label.is_some();
    let label = label.map(Into::into);
    let icon = icon.map(Into::into);
    let native_label = label.clone().unwrap_or_default();
    let native_icon = icon;

    let bg = if primary {
        rgb(0x3A3A3A)
    } else {
        rgb(0x242424)
    };
    let hover = if primary {
        rgb(0x484848)
    } else {
        rgb(0x323232)
    };
    let border = if primary {
        rgb(0x4A4A4A)
    } else {
        rgb(0x303030)
    };

    let fallback = div()
        .id(id)
        .h(gpui::rems(style::CONTROL / 16.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .border_1()
        .border_color(border)
        .rounded(px(style::RADIUS))
        .text_size(gpui::rems(style::TEXT / 16.0))
        .text_color(white())
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .when(has_label, |this| this.gap(px(6.0)))
        .when_some(icon, |this, icon| {
            this.child(lucide_icon(icon, style::ICON))
        })
        .when_some(label, |this, label| this.child(div().child(label)));
    crate::desktop::controls::button(fallback, native_label, native_icon)
}

/// Neutral action control matching table filters and compact text fields.
pub(crate) fn surface_button(
    id: impl Into<ElementId>,
    icon: Option<Icon>,
    label: Option<&str>,
) -> crate::desktop::controls::SurfaceControl {
    crate::desktop::controls::button(
        surface_button_fallback(id, icon, label),
        label.unwrap_or_default().to_owned(),
        icon,
    )
}

/// Icon-only surface control with a meaningful native tooltip and accessibility label.
pub(crate) fn surface_icon_button(
    id: impl Into<ElementId>,
    icon: Icon,
    help: &str,
) -> crate::desktop::controls::SurfaceControl {
    crate::desktop::controls::icon_button(
        surface_button_fallback(id, Some(icon), None),
        help.to_owned(),
        icon,
    )
}

fn surface_button_fallback(
    id: impl Into<ElementId>,
    icon: Option<Icon>,
    label: Option<&str>,
) -> Stateful<Div> {
    let has_label = label.is_some();
    div()
        .id(id)
        .h(rems(style::CONTROL / 16.0))
        .flex_none()
        .px(px(9.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(style::CARD_RADIUS))
        .bg(rgb(style::SURFACE))
        .border_1()
        .border_color(rgb(style::BORDER))
        .text_size(rems(style::META / 16.0))
        .text_color(rgb(style::TEXT_PRIMARY))
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| {
            s.bg(rgb(style::SURFACE_HOVER))
                .text_color(rgb(style::TEXT_SELECTED))
        })
        .when(has_label, |control| control.gap(px(6.0)))
        .when_some(icon, |control, icon| {
            control.child(lucide_icon(icon, style::ICON))
        })
        .when_some(label, |control, label| control.child(label.to_owned()))
}
