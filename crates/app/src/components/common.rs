use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use machines::machine::MachineKind;

pub type LucideIcon = Icon;

pub const LUCIDE_FONT_FAMILY: &str = "lucide";

pub fn machine_icon(kind: MachineKind) -> LucideIcon {
    match kind {
        MachineKind::MacOS => Icon::Laptop,
        MachineKind::Linux => Icon::Server,
        MachineKind::Unknown => Icon::Monitor,
    }
}

pub fn lucide_icon(icon: LucideIcon, size: f32) -> Div {
    div()
        .flex_none()
        .font_family(LUCIDE_FONT_FAMILY)
        .text_size(px(size))
        .child(char::from(icon).to_string())
}

pub fn button<L>(
    id: impl Into<ElementId>,
    icon: LucideIcon,
    label: Option<L>,
    primary: bool,
) -> Stateful<Div>
where
    L: Into<SharedString>,
{
    let has_label = label.is_some();
    let label = label.map(Into::into);
    let bg = if primary {
        rgb(0x0A84FF)
    } else {
        rgb(0x2C2C2E)
    };
    let hover = if primary {
        rgb(0x3B9CFF)
    } else {
        rgb(0x3A3A3C)
    };
    let border = if primary {
        rgb(0x0A84FF)
    } else {
        rgb(0x3A3A3C)
    };

    div()
        .id(id)
        .h(px(34.0))
        .px(px(14.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .border_1()
        .border_color(border)
        .rounded(px(8.0))
        .text_sm()
        .text_color(white())
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .when(has_label, |this| this.gap(px(8.0)))
        .child(lucide_icon(icon, 14.0))
        .when_some(label, |this, label| this.child(div().child(label)))
}
