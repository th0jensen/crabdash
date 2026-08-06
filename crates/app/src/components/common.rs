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

pub fn button<I, L>(
    id: impl Into<ElementId>,
    icon: Option<I>,
    label: Option<L>,
    primary: bool,
) -> Stateful<Div>
where
    L: Into<SharedString>,
    I: Into<LucideIcon>,
{
    let has_label = label.is_some();
    let label = label.map(Into::into);
    let icon = icon.map(Into::into);

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

    div()
        .id(id)
        .h(px(32.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .border_1()
        .border_color(border)
        .rounded(px(4.0))
        .text_size(px(13.0))
        .text_color(white())
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .when(has_label, |this| this.gap(px(6.0)))
        .when_some(icon, |this, icon| this.child(lucide_icon(icon, 13.0)))
        .when_some(label, |this, label| this.child(div().child(label)))
}
