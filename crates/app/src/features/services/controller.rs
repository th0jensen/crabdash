use crate::app::Crabdash;
use gpui::*;
use services::Services as _;

impl Crabdash {
    pub(crate) fn refresh_system_services_for(
        &mut self,
        uuid: uuid::Uuid,
        replace: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(mut machine) = self
            .machine_store
            .machines
            .iter()
            .find(|m| m.uuid == uuid)
            .cloned()
        else {
            return;
        };
        let ticket = if replace {
            self.services_refresh.restart(uuid)
        } else {
            let Some(ticket) = self.services_refresh.begin(uuid) else {
                return;
            };
            ticket
        };
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let result = cx
                .background_spawn(async move { machine.list_services().await })
                .await;
            this.update(cx, move |this, cx| {
                if !this.services_refresh.complete(&uuid, ticket) {
                    return;
                }
                let selected = this.selected_machine().uuid == uuid;
                let Some(machine) = this
                    .machine_store
                    .machines
                    .iter_mut()
                    .find(|m| m.uuid == uuid)
                else {
                    return;
                };
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

    pub(crate) fn execute_service_action(
        &mut self,
        name: String,
        action: services::ServiceAction,
        cx: &mut Context<Self>,
    ) {
        let uuid = self.selected_machine().uuid;
        let key = (uuid, name.clone());
        if self.pending_service_actions.contains_key(&key) {
            return;
        }
        let mut machine = self.selected_machine().clone();
        self.pending_service_actions.insert(key.clone(), action);
        if let Some(service) = self
            .selected_machine_mut()
            .services
            .systemd
            .iter_mut()
            .find(|s| s.name == name)
        {
            service.error = None;
        }
        let ticket = self.services_refresh.restart(uuid);
        let action_ticket = self.service_action_requests.restart(key.clone());
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let result = cx.background_spawn(async move { machine.service_action(&name, action).await }).await;
            this.update(cx, move |this, cx| {
                if !this.service_action_requests.complete(&key, action_ticket) { return; }
                this.pending_service_actions.remove(&key);
                let selected = this.selected_machine().uuid == uuid;
                match result {
                    Ok(_) => this.refresh_system_services_for(uuid, true, cx),
                    Err(error) => {
                        this.services_refresh.complete(&uuid, ticket);
                        let name = &key.1;
                        let message = format!("Failed to {} {name}: {error}", action.command());
                        tracing::warn!(%error, action = action.command(), service = %name, "Service action failed");
                        if let Some(machine) = this.machine_store.machines.iter_mut().find(|m| m.uuid == uuid) {
                            if let Some(service) = machine.services.systemd.iter_mut().find(|s| &s.name == name) {
                                service.error = Some(message.clone());
                            }
                            if selected { this.set_status_error(message); }
                        }
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
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
            this.service_log_refresh.forget(&log_key);
            cx.notify();
            return;
        }

        let Some(mut machine) = this
            .machine_store
            .machines
            .iter()
            .find(|m| m.uuid == log_key.0)
            .cloned()
        else {
            return;
        };
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
            this.logs_open_services.insert(log_key.clone());

            let lines = this.preferences.log_lines;
            let ticket = this.service_log_refresh.restart(log_key.clone());
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
                        if !this.service_log_refresh.complete(&fetch_key, ticket) {
                            return;
                        }
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
