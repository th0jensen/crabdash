//! Native content specifications independent of domain actions.
use crate::components::text_field::TextField;
use gpui::{Entity, Rgba, SharedString};
use lucide_icons::Icon;
#[derive(Clone, Copy)]
pub(crate) enum ButtonRole {
    Normal,
    Toolbar,
    Primary,
    Destructive,
}
#[derive(Clone)]
pub(crate) enum Spec {
    Button {
        label: SharedString,
        accessibility: SharedString,
        icon: Option<Icon>,
        selected: Option<bool>,
        role: ButtonRole,
    },
    Search {
        input: Entity<TextField>,
        placeholder: SharedString,
    },
    Popup {
        input: Entity<TextField>,
        values: Vec<String>,
        help: SharedString,
    },
    Switch {
        label: SharedString,
        value: bool,
    },
    Status {
        label: SharedString,
        color: Rgba,
        semantic: bool,
    },
}
pub(super) enum Event {
    Action {
        stamp: super::super::events::Stamp,
        text: String,
    },
    Focus {
        stamp: super::super::events::Stamp,
    },
}
pub(super) fn symbol(icon: Icon) -> &'static str {
    match icon {
        Icon::RefreshCw | Icon::RotateCcw => "arrow.clockwise",
        Icon::Pause => "pause.fill",
        Icon::ArrowUp => "arrow.up",
        Icon::ArrowDown => "arrow.down",
        Icon::ArrowDownUp => "arrow.up.arrow.down",
        Icon::Play => "play.fill",
        Icon::Square => "stop.fill",
        Icon::Trash2 => "trash",
        Icon::FileText => "doc.text",
        Icon::Copy => "doc.on.doc",
        Icon::Plus => "plus",
        Icon::X => "xmark",
        Icon::Info => "info.circle",
        Icon::ChevronDown => "chevron.down",
        Icon::ChevronRight => "chevron.right",
        Icon::Pencil => "pencil",
        Icon::SquareCheck => "checkmark.square",
        _ => "circle",
    }
}

pub(super) fn button_symbol(icon: Icon, selected: Option<bool>) -> &'static str {
    if matches!(icon, Icon::Square) && selected.is_some() {
        "square"
    } else {
        symbol(icon)
    }
}
