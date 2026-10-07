use super::Key;
use crate::{app::Crabdash, features::polling::Target};
use gpui::*;
use services::docker::Docker;

impl Crabdash {
    pub(crate) fn toggle_docker_details(&mut self, key: Key, cx: &mut Context<Self>) {
        if self.docker_details.is_open(&key) {
            self.docker_details.close(&key);
            cx.notify();
            return;
        }
        self.load_docker_details(key, cx);
    }

    pub(crate) fn refresh_docker_details(&mut self, key: Key, cx: &mut Context<Self>) {
        if self.docker_details.is_open(&key) {
            self.load_docker_details(key, cx);
        }
    }

    fn load_docker_details(&mut self, key: Key, cx: &mut Context<Self>) {
        let Some(mut machine) = self
            .machine_store
            .machines
            .iter()
            .find(|machine| {
                machine.uuid == key.0
                    && machine
                        .services
                        .docker
                        .iter()
                        .any(|container| container.id == key.1)
            })
            .cloned()
        else {
            self.docker_details.close(&key);
            return;
        };
        let target = Target::from_machine(&machine);
        let ticket = self.docker_details.begin(key.clone());
        self.docker_details
            .record_target(key.clone(), target.clone());
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let id = key.1.clone();
            let result = cx
                .background_spawn(async move {
                    machine
                        .inspect_container(&id)
                        .await
                        .map_err(|error| format!("Unable to load container details: {error}"))
                })
                .await;
            this.update(cx, move |this, cx| {
                let valid = this.machine_store.machines.iter().any(|machine| {
                    target.matches(machine)
                        && machine
                            .services
                            .docker
                            .iter()
                            .any(|container| container.id == key.1)
                });
                if !valid {
                    if this.docker_details.discard(&key, ticket)
                        && this.selected_machine().uuid == key.0
                    {
                        cx.notify();
                    }
                    return;
                }
                if this.docker_details.complete(&key, ticket, result)
                    && this.selected_machine().uuid == key.0
                {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}
