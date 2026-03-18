use std::{fs, path::PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use services::MachineServices;
use uuid::Uuid;

use crate::{
    commands::SystemInfo,
    machine::{Machine, MachineKind},
    remote_connection::AuthMethod,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct MachineStore {
    pub machines: Vec<Machine>,
}

fn machines_file_path() -> Result<PathBuf> {
    let mut path = dirs::data_local_dir().context("Could not determine local data directory")?;

    path.push("com.thojensen.crabdash");
    path.push("machines.json");

    Ok(path)
}

impl MachineStore {
    pub fn load() -> Result<Self> {
        let path = machines_file_path()?;

        if !path.exists() {
            let store = MachineStore::default();
            store.save()?;
            return Ok(store);
        };

        let contents = fs::read_to_string(&path)?;
        let store = serde_json::from_str(&contents)?;
        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let path = machines_file_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
    /// Updates an existing machine in the store by matching on `uuid`, then
    /// persists the change to disk.
    ///
    /// If no machine with a matching `uuid` is found, the store is saved
    /// unchanged. Use [`MachineStore::add_machine`] to insert new machines.
    ///
    /// # Returns
    /// * `Ok(())`: The store was successfully saved
    /// * `Err(anyhow::Error)`: If loading or saving the store fails
    pub fn save_machine(mc: Machine) -> Result<()> {
        let mut store = Self::load()?;
        let existing = store
            .machines
            .iter_mut()
            .find(|m| m.uuid == mc.uuid)
            .ok_or_else(|| anyhow!("No machine found with id '{}'", mc.id))?;
        *existing = mc;
        store.save()
    }
    /// Removes an existing machine from the store by matching on `uuid`, then
    /// persists the change to disk.
    ///
    /// If no machine with a matching `uuid` is found, the store is saved
    /// unchanged.
    ///
    /// # Returns
    /// * `Ok(())`: The store was successfully saved
    /// * `Err(anyhow::Error)`: If loading or saving the store fails
    pub fn remove_machine(mc: Machine) -> Result<()> {
        let mut store = Self::load()?;
        let index = store
            .machines
            .iter()
            .position(|m| m.uuid == mc.uuid)
            .ok_or_else(|| anyhow!("No machine found with id '{}'", mc.id))?;
        store.machines.remove(index);
        store.save()
    }
    /// Appends a machine to the store and persists it to disk.
    ///
    /// The push is rolled back if saving fails, keeping in-memory and on-disk
    /// state consistent.
    ///
    /// # Returns
    /// * `Ok(usize)`: The index of the newly added machine
    /// * `Err(anyhow::Error)`: If saving the store fails
    pub fn add_machine(&mut self, machine: Machine) -> Result<usize> {
        self.machines.push(machine);
        let index = self.machines.len() - 1;

        if let Err(error) = self.save() {
            self.machines.pop();
            return Err(error);
        }

        Ok(index)
    }
    /// Connects to a remote machine via SSH and adds it to the store.
    ///
    /// Convenience wrapper around [`Machine::new_remote`] and
    /// [`MachineStore::add_machine`].
    ///
    /// # Returns
    /// * `Ok(usize)`: The index of the newly added machine
    /// * `Err(anyhow::Error)`: If the SSH connection fails or saving the store fails
    pub fn add_remote_machine(
        &mut self,
        user: String,
        host: String,
        password: AuthMethod,
    ) -> Result<usize> {
        let machine = Machine::new_remote(&user, &host, password)?;
        self.add_machine(machine)
    }
}

impl Default for MachineStore {
    /// Creates a default [`MachineStore`] pre-populated with a single local
    /// machine with the id `"localhost"`.
    ///
    /// # Panics
    /// Panics if local system information cannot be retrieved.
    fn default() -> Self {
        let sys = SystemInfo::local().expect("Failed to retrieve SystemInfo");
        let kind = MachineKind::get_kind(&sys);
        MachineStore {
            machines: vec![Machine {
                uuid: Uuid::new_v4(),
                id: "localhost".to_string(),
                system_info: sys,
                kind,
                remote: None,
                docker_path: None,
                services: MachineServices::default(),
            }],
        }
    }
}
