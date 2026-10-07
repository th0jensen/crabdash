use crate::app::Crabdash;
use gpui::*;
use services::docker::Docker;
use services::docker::DockerAction;
use uuid::Uuid;

impl Crabdash {
    pub(crate) fn open_docker_run_modal(&mut self, cx: &mut Context<Self>) {
        self.docker_run_modal_open = true;
        cx.notify();
    }

    pub(crate) fn close_docker_run_modal(&mut self, cx: &mut Context<Self>) {
        if self.docker_run_config.busy {
            return;
        }
        self.docker_run_modal_open = false;
        cx.notify();
    }

    pub(crate) fn submit_docker_run(&mut self, cx: &mut Context<Self>) {
        if self.docker_run_config.busy {
            return;
        }
        let args = match self.docker_run_config.build_args(cx) {
            Ok(args) => args,
            Err(error) => {
                self.docker_run_config.error = Some(error);
                cx.notify();
                return;
            }
        };
        let mut machine = self.selected_machine().clone();
        let machine_uuid = machine.uuid;
        self.docker_run_config.busy = true;
        self.docker_run_config.error = None;
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let result = cx
                .background_spawn(async move {
                    match machine.run_container(&args).await {
                        Ok(_) => Ok(machine.list_docker().await),
                        Err(error) => Err(error),
                    }
                })
                .await;
            this.update(cx, move |this, cx| {
                this.docker_run_config.busy = false;
                match result {
                    Ok(containers) => {
                        if let Some(machine) = this
                            .machine_store
                            .machines
                            .iter_mut()
                            .find(|m| m.uuid == machine_uuid)
                        {
                            match containers {
                                Ok(containers) => {
                                    machine.services.docker = containers;
                                    machine.services.docker_error = None;
                                }
                                Err(error) => {
                                    machine.services.docker_error =
                                        Some(format!("Unable to refresh Docker: {error}"));
                                }
                            }
                        }
                        this.docker_run_modal_open = false;
                        this.docker_run_config.reset(cx);
                        this.clear_status_message();
                    }
                    Err(error) => {
                        this.docker_run_config.error =
                            Some(format!("Docker could not run the container: {error}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn refresh_docker(&mut self, cx: &mut Context<Self>) {
        let machine = self.selected_machine().clone();
        let machine_index = self.selected_machine;
        cx.spawn({
            let mut machine = machine.clone();
            async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                let result = cx
                    .background_spawn(async move { machine.list_docker().await })
                    .await;
                this.update(cx, move |this, cx| {
                    if let Some(machine) = this.machine_store.machines.get_mut(machine_index) {
                        match result {
                            Ok(containers) => {
                                machine.services.docker = containers;
                                machine.services.docker_error = None;
                            }
                            Err(error) => {
                                let message = format!("Unable to load Docker: {error}");
                                let newly_failed =
                                    machine.services.docker_error.as_ref() != Some(&message);
                                machine.services.docker_error = Some(message.clone());
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

    pub(crate) fn execute_docker_action(
        &mut self,
        machine_uuid: Uuid,
        id: String,
        action: DockerAction,
        cx: &mut Context<Self>,
    ) {
        let key = (machine_uuid, id.clone());
        if self.pending_docker_actions.contains_key(&key) {
            return;
        }
        let Some(machine) = self
            .machine_store
            .machines
            .iter_mut()
            .find(|m| m.uuid == machine_uuid)
        else {
            return;
        };
        let Some(container) = machine.services.docker.iter_mut().find(|c| c.id == id) else {
            return;
        };
        if !action.allowed_for(container) {
            return;
        }
        let name = container.name.clone();
        container.error = None;
        let mut machine = machine.clone();
        self.pending_docker_actions.insert(key.clone(), action);
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let bg_id = id.clone();
            let result = cx
                .background_spawn(async move {
                    machine.container_action(&bg_id, action).await?;
                    Ok::<_, anyhow::Error>(machine.list_docker().await)
                })
                .await;
            this.update(cx, move |this, cx| {
                this.pending_docker_actions.remove(&key);
                let Some(machine) = this
                    .machine_store
                    .machines
                    .iter_mut()
                    .find(|m| m.uuid == machine_uuid)
                else {
                    return;
                };
                match result {
                    Ok(refresh) => {
                        if matches!(action, DockerAction::Remove { .. }) {
                            machine.services.docker.retain(|c| c.id != id);
                            this.logs_open_containers.remove(&key);
                            this.expanded_docker_logs.remove(&key);
                        }
                        match refresh {
                            Ok(containers) => {
                                machine.services.docker = containers;
                                machine.services.docker_error = None;
                                this.clear_status_message();
                            }
                            Err(err) => {
                                let message = format!(
                                    "{} completed for {name}, but refreshing Docker failed: {err}",
                                    action.label()
                                );
                                machine.services.docker_error = Some(message.clone());
                                this.set_status_error(message);
                            }
                        }
                    }
                    Err(err) => {
                        let message =
                            format!("Failed to {} {name}: {err}", action.label().to_lowercase());
                        if let Some(container) =
                            machine.services.docker.iter_mut().find(|c| c.id == id)
                        {
                            container.error = Some(message.clone());
                        }
                        this.set_status_error(message);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn toggle_docker_logs(&mut self, log_key: (Uuid, String), cx: &mut Context<Self>) {
        let this = self;
        let id = log_key.1.clone();

        if this.logs_open_containers.contains(&log_key) {
            this.logs_open_containers.remove(&log_key);
            cx.notify();
            return;
        } else {
            this.logs_open_containers.insert(log_key.clone());
        }

        {
            let state = match crate::features::terminal::TerminalState::new_log(
                500,
                this.preferences.log_lines as u16,
            ) {
                Ok(s) => s,
                Err(err) => {
                    this.set_status_error(format!("Failed to init terminal: {err}"));
                    cx.notify();
                    return;
                }
            };
            this.expanded_docker_logs.insert(log_key.clone(), state);

            let lines = this.preferences.log_lines;
            let mut machine = this.selected_machine().clone();
            let fetch_id = id.clone();
            let fetch_key = log_key.clone();
            cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let bg_id = fetch_id.clone();
                    let result = cx
                        .background_spawn(
                            async move { machine.container_logs(&bg_id, lines).await },
                        )
                        .await;
                    this.update(&mut cx, move |this, cx| {
                        if let Some(state) = this.expanded_docker_logs.get_mut(&fetch_key) {
                            match result {
                                Ok(logs) => state.feed(logs),
                                Err(err) => state.feed_string(format!("Error: {err}")),
                            }
                        }
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();
        }

        cx.notify();
    }
}
