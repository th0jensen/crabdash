use super::Domains;
use crate::{app::Crabdash, features::workspaces::model::Tab};
use gpui::Context;
use std::time::Instant;

impl Crabdash {
    pub(crate) fn prepare_visible_domains(&mut self, cx: &mut Context<Self>) {
        for machine in &self.machine_store.machines {
            self.polling.publish_connection(machine);
        }
        let domains = Domains::visible(&self.workspaces.layout().root);
        if self.polling.replaced_target(self.selected_machine())
            || self.polling.metadata_replaced(self.selected_machine())
        {
            // UUID alone cannot distinguish a replaced endpoint/session. Invalidate
            // its old tickets so newly visible reads cannot coalesce into stale work.
            let uuid = self.selected_machine().uuid;
            self.docker_refresh.forget(&uuid);
            self.disks_refresh.forget(&uuid);
            self.services_refresh.forget(&uuid);
            self.machine_refresh.forget(&uuid);
        }
        // Commit visibility before launching tasks that can immediately notify.
        let newly_visible = self
            .polling
            .observe(&self.machine_store.machines[self.selected_machine], domains);
        self.refresh_domains(newly_visible, cx);
        if self
            .polling
            .metadata_due(self.selected_machine(), Instant::now())
        {
            self.sync_state_for(self.selected_machine().uuid, cx);
        }
    }

    fn refresh_domains(&mut self, domains: Domains, cx: &mut Context<Self>) {
        let uuid = self.selected_machine().uuid;
        if domains.contains(Tab::Docker) {
            self.refresh_docker_for(uuid, false, cx);
        }
        if domains.contains(Tab::Disks) {
            self.refresh_disks_for(uuid, cx);
        }
        if domains.contains(Tab::Services) {
            self.refresh_system_services_for(uuid, false, cx);
        }
    }

    pub(crate) fn refresh_visible_tables(&mut self, cx: &mut Context<Self>) {
        let domains = Domains::visible(&self.workspaces.layout().root);
        self.refresh_domains(domains, cx);
        if self
            .polling
            .metadata_due(self.selected_machine(), Instant::now())
        {
            self.sync_state_for(self.selected_machine().uuid, cx);
        }
    }
}
