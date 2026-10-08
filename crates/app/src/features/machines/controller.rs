use super::AddMachineAuthMode;
use super::sidebar;
use crate::app::Crabdash;
use crate::components::text_field::{FieldTab, FieldTabPrev};
use anyhow::anyhow;
use gpui::*;
use machines::{
    machine::Machine,
    remote_connection::AuthMethod,
    store::{MachineStore, load_store},
};
use std::path::PathBuf;
use uuid::Uuid;

impl Crabdash {
    pub(crate) fn selected_machine(&self) -> &Machine {
        &self.machine_store.machines[self.selected_machine]
    }

    pub(crate) fn selected_machine_mut(&mut self) -> &mut Machine {
        &mut self.machine_store.machines[self.selected_machine]
    }

    pub(crate) fn select_machine(
        &mut self,
        uuid: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(machine) = self
            .machine_store
            .machines
            .iter()
            .find(|machine| machine.uuid == uuid)
        else {
            return;
        };
        let credentials_key = machine
            .remote
            .as_ref()
            .filter(|remote| super::authentication::needs_stored_secret(remote))
            .map(|remote| format!("com.thojensen.crabdash.ssh.{}@{}", remote.user, remote.host));

        self.machine_selection_generation = self.machine_selection_generation.wrapping_add(1);
        let generation = self.machine_selection_generation;
        window.activate_window();
        cx.spawn_in(window, async move |this: WeakEntity<Crabdash>, cx| {
            let credentials = if let Some(key) = credentials_key {
                match cx.update(|_, app| app.read_credentials(&key)) {
                    Ok(credentials) => credentials.await.ok().flatten(),
                    Err(_) => None,
                }
            } else {
                None
            };

            this.update_in(cx, |this, window, cx| {
                if this.machine_selection_generation != generation {
                    return;
                }
                let Some(index) = this
                    .machine_store
                    .machines
                    .iter()
                    .position(|m| m.uuid == uuid)
                else {
                    return;
                };
                if let Some((_, bytes)) = credentials
                    && let Some(remote) = this
                        .machine_store
                        .machines
                        .get_mut(index)
                        .and_then(|machine| machine.remote.as_mut())
                    && let Some(auth) = remote.auth.as_mut()
                {
                    auth.apply_secret(String::from_utf8_lossy(&bytes).into());
                }

                this.selected_machine = index;
                this.refresh_services(cx);
                this.focus_selected_terminal(window, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn sync_state_for(&mut self, uuid: Uuid, cx: &mut Context<Self>) {
        let Some(mut machine) = self
            .machine_store
            .machines
            .iter()
            .find(|m| m.uuid == uuid)
            .cloned()
        else {
            return;
        };
        if self.polling.metadata_replaced(&machine) {
            self.machine_refresh.forget(&uuid);
        }
        let Some(ticket) = self.machine_refresh.begin(uuid) else {
            return;
        };
        let target = crate::features::polling::Target::from_machine(&machine);
        let requested = std::time::Instant::now();
        self.polling.record_metadata_request(&machine, requested);
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            let result = cx
                .background_spawn(async move {
                    let changed = machine.sync_system_info().await?;
                    Ok::<_, anyhow::Error>((machine.system_info, machine.kind, changed))
                })
                .await;
            this.update(cx, |this, cx| {
                if !this.machine_refresh.complete(&uuid, ticket) {
                    return;
                }
                match result {
                    Ok((info, kind, changed)) => {
                        if let Some(machine) = this
                            .machine_store
                            .machines
                            .iter_mut()
                            .find(|m| m.uuid == uuid)
                        {
                            if !target.matches(machine) {
                                return;
                            }
                            if changed {
                                machine.system_info = info;
                                machine.kind = kind;
                            }
                            // Newly discovered platform metadata must not cause
                            // another TTL request on the immediately following tick.
                            this.polling.record_metadata_request(machine, requested);
                            if changed {
                                cx.notify();
                            }
                        }
                    }
                    Err(error) => tracing::warn!(%error, "sync_system_info failed"),
                }
            })
            .ok();
        })
        .detach();
    }

    fn replace_machine_store(&mut self, mut store: MachineStore) {
        let selected = self.selected_machine().uuid;
        self.selected_machine = super::model::reconcile(&mut store, &self.machine_store, selected);
        self.machine_store = store;
    }

    pub(crate) fn start_update_loop(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Crabdash>, cx: &mut AsyncApp| {
            loop {
                let interval = match this.update(cx, |this, _| this.preferences.refresh_seconds) {
                    Ok(interval) => interval,
                    Err(_) => break,
                };
                smol::Timer::after(std::time::Duration::from_secs(interval)).await;
                // Heartbeats also run with table auto-refresh disabled. Snapshot
                // only connection owners, avoiding clones of cached table data.
                let machines = match this.update(cx, |this, _cx| {
                    this.machine_store
                        .machines
                        .iter()
                        .map(|machine| {
                            let mut remote = machine.remote.clone();
                            if let Some(remote) = remote.as_mut() {
                                remote.auth = None;
                            }
                            (
                                machine.uuid,
                                crate::features::polling::Target::from_machine(machine),
                                remote,
                            )
                        })
                        .collect::<Vec<_>>()
                }) {
                    Ok(m) => m,
                    Err(_) => break,
                };

                // Check connected state for each remote machine asynchronously
                let mut connected_states = Vec::with_capacity(machines.len());
                for (uuid, target, remote) in machines {
                    let state = match remote.as_ref() {
                        Some(rc) => rc.has_active_session().await,
                        None => true,
                    };
                    connected_states.push((uuid, target, state));
                }

                // Apply connected states (shared Arc, so clones see this too) and refresh
                this.update(cx, |this, cx| {
                    let mut changed = false;
                    for (uuid, target, connected) in connected_states {
                        if let Some(m) = this
                            .machine_store
                            .machines
                            .iter_mut()
                            .find(|m| m.uuid == uuid)
                        {
                            if !target.matches(m) {
                                continue;
                            }
                            if let Some(rc) = m.remote.as_ref() {
                                rc.set_connected(connected);
                            }
                            changed |= this.polling.publish_connection(m);
                        }
                    }
                    if this.preferences.auto_refresh {
                        this.refresh_visible_tables(cx);
                    }
                    if changed {
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    pub(crate) fn delete_machine(
        &mut self,
        uuid: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(%uuid, "delete_machine called");
        cx.spawn_in(window, async move |this: WeakEntity<Crabdash>, cx| {
            if let Err(error) = MachineStore::remove_machine(uuid).await {
                this.update_in(cx, |this, _, cx| {
                    this.set_status_error(format!("Unable to delete machine: {error}"));
                    cx.notify();
                })
                .ok();
                return;
            }
            match load_store().await {
                Ok(store) => {
                    this.update_in(cx, |this, window, cx| {
                        if let Some(drawer) = this.quake_terminals.remove(&uuid) {
                            drawer.shutdown();
                        }
                        let previous = this.selected_machine().uuid;
                        this.replace_machine_store(store);
                        if this.docker_run_config.cancel_run_for(uuid) {
                            this.docker_run_modal_open = false;
                        }
                        this.system.remove(uuid);
                        this.docker_details.remove_machine(uuid);
                        this.machine_refresh.forget(&uuid);
                        this.polling.forget(uuid);
                        this.disks_refresh.forget(&uuid);
                        this.services_refresh.forget(&uuid);
                        this.docker_refresh.forget(&uuid);
                        this.docker_log_refresh.forget_where(|key| key.0 == uuid);
                        this.service_log_refresh.forget_where(|key| key.0 == uuid);
                        this.docker_action_requests
                            .forget_where(|key| key.0 == uuid);
                        this.service_action_requests
                            .forget_where(|key| key.0 == uuid);
                        this.pending_service_actions.retain(|key, _| key.0 != uuid);
                        this.pending_docker_actions.retain(|key, _| key.0 != uuid);
                        this.logs_open_services.retain(|key| key.0 != uuid);
                        this.logs_open_containers.retain(|key| key.0 != uuid);
                        this.expanded_service_logs.retain(|key, _| key.0 != uuid);
                        this.expanded_docker_logs.retain(|key, _| key.0 != uuid);
                        this.clear_status_message();
                        this.refresh_services(cx);
                        this.reconcile_selected_terminal(previous, window, cx);
                        cx.notify();
                    })
                    .ok();
                }
                Err(error) => {
                    this.update_in(cx, |this, _, cx| {
                        this.set_status_error(format!(
                            "Unable to reload machines after delete: {error}"
                        ));
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    pub(crate) fn open_add_machine_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_machine_modal_open = true;
        self.add_machine_error = None;
        window.focus(&self.remote_host_field.focus_handle(cx));
        cx.notify();
    }

    pub(crate) fn close_add_machine_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_machine_modal_open = false;
        self.add_machine_error = None;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(crate) fn set_add_machine_auth_mode(
        &mut self,
        mode: AddMachineAuthMode,
        cx: &mut Context<Self>,
    ) {
        self.add_machine_auth_mode = mode;
        self.add_machine_error = None;
        cx.notify();
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        self.sidebar_collapsed = !self.sidebar_collapsed;
        self.persist_workspace(cx);
    }

    pub(crate) fn set_sidebar_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        self.sync_workspace_store(cx);
        self.sidebar_width = sidebar::clamp_width(width);
        self.persist_workspace(cx);
    }

    fn clear_remote_machine_form(&mut self, cx: &mut Context<Self>) {
        self.remote_host_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_user_field
            .update(cx, |field, cx| field.clear(cx));
        self.add_machine_auth_mode = AddMachineAuthMode::Password;
        self.remote_password_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_private_key_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_public_key_field
            .update(cx, |field, cx| field.clear(cx));
        self.remote_passphrase_field
            .update(cx, |field, cx| field.clear(cx));
    }

    pub(crate) fn submit_add_machine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_machine_error = None;

        let host = self.remote_host_field.read(cx).text().trim().to_string();
        let user = self.remote_user_field.read(cx).text().trim().to_string();
        let auth = match self.add_machine_auth_mode {
            AddMachineAuthMode::None => Ok(AuthMethod::None),
            AddMachineAuthMode::Password => {
                if !self.remote_password_field.read(cx).text().is_empty() {
                    let password = self.remote_password_field.read(cx).text();
                    Ok(AuthMethod::Password(password))
                } else {
                    Err(anyhow!("Host, user, and password are required."))
                }
            }
            AddMachineAuthMode::AuthKey => {
                let private_key = self
                    .remote_private_key_field
                    .read(cx)
                    .text()
                    .trim()
                    .to_string();
                let public_key = self
                    .remote_public_key_field
                    .read(cx)
                    .text()
                    .trim()
                    .to_string();
                let passphrase = self
                    .remote_passphrase_field
                    .read(cx)
                    .text()
                    .trim()
                    .to_string();

                if private_key.is_empty() {
                    Err(anyhow!("Host, user, and private key are required."))
                } else {
                    Ok(AuthMethod::AuthKey {
                        pubkey: (!public_key.is_empty()).then(|| PathBuf::from(public_key)),
                        privatekey: PathBuf::from(private_key),
                        passphrase: (!passphrase.is_empty()).then_some(passphrase),
                    })
                }
            }
        };

        if host.is_empty() || user.is_empty() {
            let error = anyhow!("Host and user are required.");
            self.set_status_error(error.to_string());
            self.add_machine_error = Some(error);
            cx.notify();
            return;
        }

        let auth = match auth {
            Ok(auth) => auth,
            Err(error) => {
                self.set_status_error(error.to_string());
                self.add_machine_error = Some(error);
                cx.notify();
                return;
            }
        };

        let submitted = super::model::Selection {
            uuid: self.selected_machine().uuid,
            generation: self.machine_selection_generation,
        };
        cx.spawn_in(
            window,
            async move |this: WeakEntity<Crabdash>, cx| -> Result<()> {
                let mut store = match load_store().await {
                    Ok(store) => store,
                    Err(error) => {
                        this.update_in(cx, move |this, _, cx| {
                            this.set_status_error(format!("Unable to load machines: {error}"));
                            this.add_machine_error = Some(error);
                            cx.notify();
                        })
                        .ok();
                        return Ok(());
                    }
                };
                match store.add_remote_machine(user, host, auth).await {
                    Ok(index) => {
                        this.update_in(cx, move |this, window, cx| {
                            let added = store.machines[index].uuid;
                            let previous = this.selected_machine().uuid;
                            let select_added =
                                submitted.permits_added_selection(super::model::Selection {
                                    uuid: previous,
                                    generation: this.machine_selection_generation,
                                });
                            this.replace_machine_store(store);
                            if select_added
                                && let Some(index) = this
                                    .machine_store
                                    .machines
                                    .iter()
                                    .position(|m| m.uuid == added)
                            {
                                this.selected_machine = index;
                                this.machine_selection_generation =
                                    this.machine_selection_generation.wrapping_add(1);
                            }
                            this.add_machine_modal_open = false;
                            this.clear_remote_machine_form(cx);
                            this.clear_status_message();
                            this.refresh_services(cx);
                            this.reconcile_selected_terminal(previous, window, cx);

                            if let Some(rc) = this
                                .machine_store
                                .machines
                                .iter()
                                .find(|machine| machine.uuid == added)
                                .and_then(|machine| machine.remote.as_ref())
                            {
                                let (key, user, auth) = (
                                    format!("com.thojensen.crabdash.ssh.{}@{}", rc.user, rc.host),
                                    rc.user.clone(),
                                    rc.auth.clone(),
                                );
                                if let Some(secret) = auth.and_then(|auth| auth.secret_bytes()) {
                                    cx.spawn(async move |_, cx| {
                                        if let Some(future) = cx
                                            .update(|app| {
                                                app.write_credentials(&key, &user, &secret)
                                            })
                                            .ok()
                                        {
                                            future.await.ok();
                                        }
                                    })
                                    .detach();
                                }
                            }
                            cx.notify();
                        })
                        .ok();
                        Ok(())
                    }
                    Err(err) => {
                        this.update_in(cx, move |this, _, cx| {
                            this.set_status_error(err.to_string());
                            this.add_machine_error = Some(err);
                            cx.notify();
                        })
                        .ok();
                        Err(anyhow::anyhow!("Failed to create machine"))
                    }
                }
            },
        )
        .detach();
    }

    pub(crate) fn focus_next(
        &mut self,
        _: &FieldTab,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.add_machine_modal_open {
            window.focus_next();
        }
    }

    pub(crate) fn focus_prev(
        &mut self,
        _: &FieldTabPrev,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.add_machine_modal_open {
            window.focus_prev();
        }
    }
}
