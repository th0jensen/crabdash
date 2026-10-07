use crate::{
    app::Crabdash,
    features::workspaces::model::{Node, Tab},
};
use gpui::*;
use uuid::Uuid;

fn system_is_visible(node: &Node) -> bool {
    match node {
        Node::Pane { active, .. } => *active == Tab::System,
        Node::Split { first, second, .. } => system_is_visible(first) || system_is_visible(second),
    }
}

impl Crabdash {
    /// Render calls this to start sampling immediately after tab/machine changes.
    pub(crate) fn prepare_system_resources(&mut self, cx: &mut Context<Self>) {
        let invalid: Vec<_> = self
            .system
            .machines
            .iter()
            .filter_map(|(uuid, state)| {
                let machine = self
                    .machine_store
                    .machines
                    .iter()
                    .find(|machine| machine.uuid == *uuid);
                let valid = machine.is_some_and(|machine| {
                    state
                        .target
                        .as_ref()
                        .is_none_or(|target| target.matches(machine))
                });
                (!valid).then_some(*uuid)
            })
            .collect();
        for uuid in invalid {
            self.system.remove(uuid);
        }
        let target =
            system_is_visible(&self.workspaces.layout().root).then(|| self.selected_machine().uuid);
        if self.system.visible_machine == target {
            return;
        }
        self.system.visible_machine = target;
        if let Some(uuid) = target {
            self.refresh_system_resources_for(uuid, cx);
        }
    }

    pub(crate) fn start_system_resource_loop(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(std::time::Duration::from_secs(1)).await;
                if this
                    .update(cx, |this, cx| {
                        // Resource sampling is live while visible, even if automatic
                        // table refresh is disabled in Preferences.
                        let uuid = this.selected_machine().uuid;
                        let due = this.system.machines.get(&uuid).is_none_or(|state| {
                            state.sampling_due(
                                std::time::Instant::now(),
                                this.preferences.system_refresh_interval(),
                            )
                        });
                        if due && system_is_visible(&this.workspaces.layout().root) {
                            this.refresh_system_resources(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn refresh_system_resources(&mut self, cx: &mut Context<Self>) {
        self.refresh_system_resources_for(self.selected_machine().uuid, cx);
    }

    pub(crate) fn refresh_visible_system_resources_for(
        &mut self,
        uuid: Uuid,
        cx: &mut Context<Self>,
    ) {
        if self.selected_machine().uuid == uuid && system_is_visible(&self.workspaces.layout().root)
        {
            self.refresh_system_resources_for(uuid, cx);
        }
    }

    pub(crate) fn refresh_system_resources_for(&mut self, uuid: Uuid, cx: &mut Context<Self>) {
        let Some(mut machine) = self
            .machine_store
            .machines
            .iter()
            .find(|machine| machine.uuid == uuid)
            .cloned()
        else {
            return;
        };
        if self
            .system
            .machines
            .get(&uuid)
            .and_then(|state| state.target.as_ref())
            .is_some_and(|target| !target.matches(&machine))
        {
            self.system.remove(uuid);
        }
        let target = super::Target::from_machine(&machine);
        let Some(ticket) = self.system.requests.begin(uuid) else {
            return;
        };
        let state = self.system.machines.entry(uuid).or_default();
        state.target = Some(target.clone());
        state.loading = true;
        state.last_requested = Some(std::time::Instant::now());
        let show_initial_loading = state.usage.is_none();
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let result = cx
                .background_spawn(async move { machine.sample_resources().await })
                .await;
            this.update(cx, |this, cx| {
                if !this.system.requests.complete(&uuid, ticket) {
                    return;
                }
                if !this
                    .machine_store
                    .machines
                    .iter()
                    .find(|machine| machine.uuid == uuid)
                    .is_some_and(|machine| target.matches(machine))
                {
                    this.system.remove(uuid);
                    cx.notify();
                    return;
                }
                let state = this.system.machines.entry(uuid).or_default();
                state.loading = false;
                match result.and_then(|sample| state.record(sample)) {
                    Ok(()) => {}
                    Err(error) => {
                        state.monitor.reset();
                        state.error = Some(format!("Unable to sample resources: {error}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        // Once live data exists, no visible state changes until the sample
        // completes. Avoid rendering the same mosaic twice per interval.
        if show_initial_loading {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::test;

    #[test]
    fn live_sampling_includes_unfocused_visible_system_panes() {
        let mut layout = crate::features::workspaces::model::Layout::default();
        assert!(!system_is_visible(&layout.root));
        assert!(layout.drop_tab(
            Tab::System,
            1,
            crate::features::workspaces::model::Drop::Right
        ));
        assert!(layout.focus(1));
        assert!(system_is_visible(&layout.root));
        assert!(layout.drop_tab(
            Tab::System,
            1,
            crate::features::workspaces::model::Drop::Tab(0)
        ));
        assert!(layout.select(Tab::Docker));
        assert!(!system_is_visible(&layout.root));
    }
}
