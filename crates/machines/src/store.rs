use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use smol::fs;
use uuid::Uuid;

use crate::{machine::Machine, remote_connection::AuthMethod};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MachineStore {
    pub machines: Vec<Machine>,
}

fn machines_file_path() -> Result<PathBuf> {
    let mut path = dirs::data_local_dir().context("Could not determine local data directory")?;

    path.push("com.thojensen.crabdash");
    path.push("machines.json");

    Ok(path)
}

pub async fn load_store() -> Result<MachineStore> {
    MachineStore::load().await
}

impl MachineStore {
    pub async fn load() -> Result<Self> {
        let path = machines_file_path()?;

        if !path.exists() {
            let store = Self::default();
            store.save().await?;
            return Ok(store);
        };

        let contents = fs::read_to_string(&path).await?;
        let store = serde_json::from_str(&contents)?;
        Ok(store)
    }

    pub async fn save(&self) -> Result<()> {
        let path = machines_file_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json).await?;
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
    pub async fn update_machine(mc: Machine) -> Result<()> {
        let mut store = load_store().await?;
        let existing = store
            .machines
            .iter_mut()
            .find(|m| m.uuid == mc.uuid)
            .ok_or_else(|| anyhow!("No machine found with id '{}'", mc.id))?;
        *existing = mc;
        store.save().await
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
    pub async fn remove_machine(uuid: Uuid) -> Result<()> {
        let mut store = load_store().await?;
        let index = store
            .machines
            .iter()
            .position(|m| m.uuid == uuid)
            .ok_or_else(|| anyhow!("No machine found with uuid '{}'", uuid))?;
        store.machines.remove(index);
        store.save().await
    }
    /// Appends a machine to the store and persists it to disk.
    ///
    /// The push is rolled back if saving fails, keeping in-memory and on-disk
    /// state consistent.
    ///
    /// # Returns
    /// * `Ok(usize)`: The index of the newly added machine
    /// * `Err(anyhow::Error)`: If saving the store fails
    pub async fn create_machine(&mut self, machine: Machine) -> Result<usize> {
        self.machines.push(machine);
        let index = self.machines.len() - 1;

        self.save().await.inspect_err(|_| {
            self.machines.pop();
        })?;

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
    pub async fn add_remote_machine(
        &mut self,
        user: String,
        host: String,
        auth: AuthMethod,
    ) -> Result<usize> {
        let machine = Machine::new_remote(&user, &host, auth).await?;
        self.create_machine(machine).await
    }
}

impl Default for MachineStore {
    /// Creates a default [`MachineStore`] pre-populated with a single local
    /// machine with the id `"localhost"`.
    fn default() -> Self {
        MachineStore {
            machines: vec![Machine::default()],
        }
    }
}
