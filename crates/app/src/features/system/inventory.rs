//! Progressive disclosure keeps device inventories out of the summary mosaic.
use crate::{
    app::Crabdash,
    components::{common::surface_button, style},
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Network,
    Disks,
}

impl Kind {
    fn id(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Disks => "disks",
        }
    }

    fn label(self, count: usize) -> &'static str {
        match (self, count) {
            (Self::Network, 1) => "interface",
            (Self::Network, _) => "interfaces",
            (Self::Disks, 1) => "device",
            (Self::Disks, _) => "devices",
        }
    }
}

#[derive(Default)]
struct Expanded {
    network: bool,
    disks: bool,
}

#[derive(Default)]
pub(super) struct State {
    machines: HashMap<Uuid, Expanded>,
}

impl State {
    fn expanded(&self, machine: Uuid, kind: Kind) -> bool {
        self.machines.get(&machine).is_some_and(|state| match kind {
            Kind::Network => state.network,
            Kind::Disks => state.disks,
        })
    }

    fn toggle(&mut self, machine: Uuid, kind: Kind) {
        let state = self.machines.entry(machine).or_default();
        let expanded = match kind {
            Kind::Network => &mut state.network,
            Kind::Disks => &mut state.disks,
        };
        *expanded = !*expanded;
    }

    pub(super) fn remove(&mut self, machine: Uuid) {
        self.machines.remove(&machine);
    }
}

pub(super) fn render(
    app: &Crabdash,
    kind: Kind,
    machine: Uuid,
    count: usize,
    rows: impl IntoIterator<Item = Div>,
    cx: &mut Context<Crabdash>,
) -> Div {
    let expanded = app.system.inventory.expanded(machine, kind);
    let label = format!(
        "{} {count} {}",
        if expanded { "Hide" } else { "Show" },
        kind.label(count),
    );
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(rems(10.0 / 16.0))
        .child(
            div().w_full().min_w_0().flex().child(
                surface_button(
                    SharedString::from(format!("system-inventory-{}-{machine}", kind.id())),
                    Some(if expanded {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    }),
                    Some(&label),
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    if app.selected_machine().uuid == machine {
                        app.system.inventory.toggle(machine, kind);
                        cx.notify();
                    }
                })),
            ),
        )
        // Use the dashboard's existing scroller; expanded rows keep the
        // provider's inventory order and never reorder with activity.
        .when(expanded, |this| this.children(rows))
        .text_size(rems(style::META / 16.0))
}

#[cfg(test)]
mod tests {
    use super::{Kind, State};
    use uuid::Uuid;

    #[test]
    fn inventories_are_independent_per_machine_and_device_kind() {
        let local = Uuid::new_v4();
        let remote = Uuid::new_v4();
        let mut state = State::default();
        assert!(!state.expanded(local, Kind::Network));
        assert!(!state.expanded(local, Kind::Disks));
        state.toggle(local, Kind::Network);
        assert!(state.expanded(local, Kind::Network));
        assert!(!state.expanded(local, Kind::Disks));
        assert!(!state.expanded(remote, Kind::Network));
        state.toggle(remote, Kind::Disks);
        state.toggle(local, Kind::Network);
        assert!(!state.expanded(local, Kind::Network));
        assert!(state.expanded(remote, Kind::Disks));
        state.toggle(local, Kind::Network);
        state.remove(local);
        assert!(!state.expanded(local, Kind::Network));
        assert!(state.expanded(remote, Kind::Disks));
    }
}
