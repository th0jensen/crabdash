use crate::app::Crabdash;
use gpui::*;
use services::Services as _;

impl Crabdash {
    pub(crate) fn refresh_system_services(&mut self, cx: &mut Context<Self>) {
        let machine = self.selected_machine().clone();
        let machine_index = self.selected_machine;
        cx.spawn({
            let mut machine = machine.clone();
            async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                let result = cx
                    .background_spawn(async move { machine.list_services().await })
                    .await;
                this.update(cx, move |this, cx| {
                    if let Some(machine) = this.machine_store.machines.get_mut(machine_index) {
                        match result {
                            Ok(services) => {
                                machine.services.systemd = services;
                                machine.services.systemd_error = None;
                            }
                            Err(error) => {
                                let message = format!("Unable to load Services: {error}");
                                let newly_failed =
                                    machine.services.systemd_error.as_ref() != Some(&message);
                                machine.services.systemd_error = Some(message.clone());
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

    pub(crate) fn execute_service_action(
        &mut self,
        name: String,
        action: services::ServiceAction,
        cx: &mut Context<Self>,
    ) {
        let this = self;

        let machine_index = this.selected_machine;
        let mut machine = this.selected_machine().clone();

        if let Some(machine) = this.machine_store.machines.get_mut(machine_index) {
            this.pending_service_actions.insert(name.clone(), action);
            if let Some(service) = machine
                .services
                .systemd
                .iter_mut()
                .find(|service| service.name == name)
            {
                service.error = None;
            }
        }

        cx.notify();

        let spawn_name = name.clone();
        let update_name = name.clone();

        cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        let result = cx
                            .background_spawn({
                                let service_name = spawn_name.clone();
                                async move {
                                    machine.service_action(&service_name, action).await?;
                                    machine.list_services().await
                                }
                            })
                            .await;

                        this.update(&mut cx, move |this, cx| {
                            this.pending_service_actions.remove(&update_name);
                            match result {
                                Ok(services) => {
                                    if let Some(machine) =
                                        this.machine_store.machines.get_mut(machine_index)
                                    {
                                        machine.services.systemd = services;
                                        machine.services.systemd_error = None;
                                    }
                                    this.clear_status_message();
                                }
                                Err(err) => {
                                    let message =
                                        format!("Failed to {} {update_name}: {err}", action.command());
                                    tracing::warn!(error = %err, action = action.command(), service = %update_name, "Service action failed");
                                    this.set_status_error(message.clone());
                                    if let Some(machine) =
                                        this.machine_store.machines.get_mut(machine_index)
                                    {
                                        if let Some(service) = machine
                                            .services
                                            .systemd
                                            .iter_mut()
                                            .find(|service| service.name == update_name)
                                        {
                                            service.error = Some(message);
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

    pub(crate) fn toggle_service_logs(
        &mut self,
        log_key: (uuid::Uuid, String),
        cx: &mut Context<Self>,
    ) {
        let this = self;
        let service_name = log_key.1.clone();

        if this.logs_open_services.contains(&log_key) {
            this.logs_open_services.remove(&log_key);
            cx.notify();
            return;
        }

        this.logs_open_services.insert(log_key.clone());

        {
            let state = match crate::features::terminal::TerminalState::new_log(
                500,
                this.preferences.log_lines as u16,
            ) {
                Ok(state) => state,
                Err(err) => {
                    this.set_status_error(format!("Failed to init terminal: {err}"));
                    cx.notify();
                    return;
                }
            };
            this.expanded_service_logs.insert(log_key.clone(), state);

            let lines = this.preferences.log_lines;
            let mut machine = this.selected_machine().clone();
            let fetch_service_name = service_name.clone();
            let fetch_key = log_key.clone();
            cx.spawn(move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let result = cx
                        .background_spawn({
                            let service_name = fetch_service_name.clone();
                            async move { machine.service_logs(&service_name, lines).await }
                        })
                        .await;
                    this.update(&mut cx, move |this, cx| {
                        if let Some(state) = this.expanded_service_logs.get_mut(&fetch_key) {
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
