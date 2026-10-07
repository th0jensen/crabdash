use crate::app::Crabdash;
use gpui::*;
use services::docker::Docker;
use services::docker::DockerAction;
use uuid::Uuid;

impl Crabdash {
    pub(crate) fn open_docker_run_modal(&mut self, cx: &mut Context<Self>) {
        if self.docker_run_config.busy {
            return;
        }
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
        let submission = self.docker_run_config.submission.begin(machine_uuid);
        let refresh_ticket = self.docker_refresh.restart(machine_uuid);
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
                if !this
                    .docker_run_config
                    .submission
                    .complete(machine_uuid, submission)
                {
                    return;
                }
                this.docker_run_config.busy = false;
                let refresh_current = this.docker_refresh.complete(&machine_uuid, refresh_ticket);
                if !this
                    .machine_store
                    .machines
                    .iter()
                    .any(|machine| machine.uuid == machine_uuid)
                {
                    this.docker_run_modal_open = false;
                    cx.notify();
                    return;
                }
                match result {
                    Ok(containers) => {
                        if refresh_current
                            && let Some(machine) = this
                                .machine_store
                                .machines
                                .iter_mut()
                                .find(|m| m.uuid == machine_uuid)
                        {
                            match containers {
                                Ok(containers) => {
                                    this.docker_details
                                        .retain_containers(machine_uuid, &containers);
                                    machine.services.docker = containers;
                                    machine.services.docker_error = None;
                                    machine.services.docker_not_installed = false;
                                }
                                Err(error) => {
                                    machine.services.docker_error =
                                        Some(format!("Unable to refresh Docker: {error}"));
                                }
                            }
                        }
                        this.docker_run_modal_open = false;
                        this.docker_run_config.reset(cx);
                        if !refresh_current {
                            this.refresh_docker_for(machine_uuid, true, cx);
                        }
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
        self.refresh_docker_for(self.selected_machine().uuid, false, cx);
    }

    pub(crate) fn refresh_docker_for(
        &mut self,
        machine_uuid: Uuid,
        replace: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(mut machine) = self
            .machine_store
            .machines
            .iter()
            .find(|machine| machine.uuid == machine_uuid)
            .cloned()
        else {
            return;
        };
        self.docker_details.reconcile_target(&machine);
        let ticket = if replace {
            self.docker_refresh.restart(machine_uuid)
        } else {
            let Some(ticket) = self.docker_refresh.begin(machine_uuid) else {
                return;
            };
            ticket
        };
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let (result, path) = cx
                .background_spawn(async move {
                    let result = machine.list_docker().await;
                    (result, machine.docker_path)
                })
                .await;
            this.update(cx, move |this, cx| {
                if !this.docker_refresh.complete(&machine_uuid, ticket) {
                    return;
                }
                let selected = this.selected_machine().uuid == machine_uuid;
                let Some(machine) = this
                    .machine_store
                    .machines
                    .iter_mut()
                    .find(|machine| machine.uuid == machine_uuid)
                else {
                    return;
                };
                if path.is_some() {
                    machine.docker_path = path;
                }
                match result {
                    Ok(containers) => {
                        this.docker_details
                            .retain_containers(machine_uuid, &containers);
                        machine.services.docker = containers;
                        machine.services.docker_error = None;
                        machine.services.docker_not_installed = false;
                    }
                    Err(error) if error.is::<services::docker::DockerNotInstalled>() => {
                        this.docker_details.remove_machine(machine_uuid);
                        machine.services.docker.clear();
                        machine.services.docker_error = None;
                        machine.services.docker_not_installed = true;
                        machine.docker_path = None;
                    }
                    Err(error) => {
                        let message = format!("Unable to load Docker: {error}");
                        let newly_failed = machine.services.docker_error.as_ref() != Some(&message);
                        machine.services.docker_error = Some(message.clone());
                        machine.services.docker_not_installed = false;
                        if newly_failed && selected {
                            this.set_status_error(message);
                        }
                    }
                }
                cx.notify();
            })
            .ok();
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
        let action_ticket = self.docker_action_requests.restart(key.clone());
        let refresh_ticket = self.docker_refresh.restart(machine_uuid);
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
                if !this.docker_action_requests.complete(&key, action_ticket) {
                    return;
                }
                this.pending_docker_actions.remove(&key);
                let selected = this.selected_machine().uuid == machine_uuid;
                let refresh_current = this.docker_refresh.complete(&machine_uuid, refresh_ticket);
                let Some(machine) = this
                    .machine_store
                    .machines
                    .iter_mut()
                    .find(|m| m.uuid == machine_uuid)
                else {
                    return;
                };
                let action_succeeded = result.is_ok();
                match result {
                    Ok(refresh) => {
                        if matches!(action, DockerAction::Remove { .. }) {
                            machine.services.docker.retain(|c| c.id != id);
                            this.logs_open_containers.remove(&key);
                            this.expanded_docker_logs.remove(&key);
                            this.docker_log_refresh.forget(&key);
                            this.docker_details.close(&key);
                        }
                        match refresh {
                            _ if !refresh_current => {
                                this.refresh_docker_for(machine_uuid, true, cx);
                            }
                            Ok(containers) => {
                                this.docker_details
                                    .retain_containers(machine_uuid, &containers);
                                machine.services.docker = containers;
                                machine.services.docker_error = None;
                                machine.services.docker_not_installed = false;
                            }
                            Err(err) => {
                                let message = format!(
                                    "{} completed for {name}, but refreshing Docker failed: {err}",
                                    action.label()
                                );
                                machine.services.docker_error = Some(message.clone());
                                if selected {
                                    this.set_status_error(message);
                                }
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
                        if selected {
                            this.set_status_error(message);
                        }
                    }
                }
                if action_succeeded && !matches!(action, DockerAction::Remove { .. }) {
                    this.refresh_docker_details(key, cx);
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
            this.docker_log_refresh.forget(&log_key);
            cx.notify();
            return;
        }

        let Some(mut machine) = this
            .machine_store
            .machines
            .iter()
            .find(|machine| machine.uuid == log_key.0)
            .cloned()
        else {
            return;
        };

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
            this.logs_open_containers.insert(log_key.clone());
            this.expanded_docker_logs.insert(log_key.clone(), state);
            let ticket = this.docker_log_refresh.restart(log_key.clone());

            let lines = this.preferences.log_lines;
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
                        if !this.docker_log_refresh.complete(&fetch_key, ticket)
                            || !this.logs_open_containers.contains(&fetch_key)
                            || !this
                                .machine_store
                                .machines
                                .iter()
                                .any(|machine| machine.uuid == fetch_key.0)
                        {
                            return;
                        }
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
