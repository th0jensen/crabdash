use crate::app::Crabdash;
use gpui::*;
use utils::disks::Disks as _;

impl Crabdash {
    pub(crate) fn toggle_disk_row(&mut self, disk_id: &str, cx: &mut Context<Self>) {
        if !self.expanded_disk_rows.insert(disk_id.to_string()) {
            self.expanded_disk_rows.remove(disk_id);
        }
        cx.notify();
    }

    pub(crate) fn refresh_disks(&mut self, cx: &mut Context<Self>) {
        let machine = self.selected_machine().clone();
        let machine_index = self.selected_machine;
        cx.spawn({
            let mut machine = machine.clone();
            async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                let result = cx
                    .background_spawn(async move { machine.list_disks().await })
                    .await;
                this.update(cx, move |this, cx| {
                    if let Some(machine) = this.machine_store.machines.get_mut(machine_index) {
                        match result {
                            Ok(disks) => {
                                machine.services.disks = disks;
                                machine.services.disks_error = None;
                            }
                            Err(error) => {
                                let message = format!("Unable to load Disks: {error}");
                                let newly_failed =
                                    machine.services.disks_error.as_ref() != Some(&message);
                                machine.services.disks_error = Some(message.clone());
                                if newly_failed && this.selected_machine == machine_index {
                                    this.set_status_error(message);
                                }
                            }
                        }
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}
