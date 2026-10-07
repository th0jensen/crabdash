use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use smol::{fs, lock::Mutex};
use uuid::Uuid;

use crate::{
    machine::{Machine, MachineKind, SystemInfo},
    remote_connection::AuthMethod,
};

// Every in-process reader and mutation uses the same lock. Connection and
// identity queries happen before taking it, so slow SSH cannot block the store.
static STORE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MachineStore {
    pub machines: Vec<Machine>,
}

fn machines_file_path() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .context("Could not determine local data directory")?
        .join("com.thojensen.crabdash/machines.json"))
}

pub async fn load_store() -> Result<MachineStore> {
    MachineStore::load().await
}

impl MachineStore {
    pub async fn load() -> Result<Self> {
        Self::load_from(&machines_file_path()?).await
    }

    async fn load_from(path: &Path) -> Result<Self> {
        let _guard = STORE_LOCK.lock().await;
        Self::load_unlocked(path).await
    }

    async fn load_unlocked(path: &Path) -> Result<Self> {
        let contents = match fs::read(path).await {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let store = Self::default();
                store.save_unlocked(path).await?;
                return Ok(store);
            }
            Err(error) => return Err(error).context("Unable to read saved machines"),
        };
        let store: Self =
            serde_json::from_slice(&contents).context("Unable to read saved machines")?;
        store.validate()?;
        Ok(store)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.machines.is_empty(),
            "The machine store must contain at least one machine"
        );
        let mut ids = HashSet::new();
        ensure!(
            self.machines.iter().all(|machine| ids.insert(machine.uuid)),
            "The machine store contains duplicate identifiers"
        );
        Ok(())
    }

    // Called only while STORE_LOCK is held. A failed write/rename leaves the
    // previous complete JSON file in place, and no caller publishes partial data.
    async fn save_unlocked(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let contents = serde_json::to_vec_pretty(self)?;
        fs::create_dir_all(path.parent().context("Missing machine store directory")?).await?;
        let temporary = path.with_extension(format!("json.{}.tmp", Uuid::new_v4()));
        if let Err(error) = fs::write(&temporary, contents).await {
            let _ = fs::remove_file(&temporary).await;
            return Err(error).context("Unable to save machines");
        }
        if let Err(error) = fs::rename(&temporary, path).await {
            let _ = fs::remove_file(&temporary).await;
            return Err(error).context("Unable to replace saved machines");
        }
        Ok(())
    }

    async fn mutate<T>(
        path: &Path,
        change: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<(Self, T)> {
        let _guard = STORE_LOCK.lock().await;
        let mut store = Self::load_unlocked(path).await?;
        let result = change(&mut store)?;
        store.save_unlocked(path).await?;
        Ok((store, result))
    }

    /// Update only identity fields in the latest store, preserving Docker cache
    /// and connection configuration written by other operations.
    pub async fn update_system_info(uuid: Uuid, info: SystemInfo, kind: MachineKind) -> Result<()> {
        Self::update_system_info_at(&machines_file_path()?, uuid, info, kind).await
    }

    async fn update_system_info_at(
        path: &Path,
        uuid: Uuid,
        info: SystemInfo,
        kind: MachineKind,
    ) -> Result<()> {
        Self::mutate(path, move |store| {
            let existing = store
                .machines
                .iter_mut()
                .find(|current| current.uuid == uuid)
                .ok_or_else(|| anyhow!("No machine found with uuid '{uuid}'"))?;
            existing.system_info = info;
            existing.kind = kind;
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Cache only the discovered CLI path. Deleted machines are not recreated.
    pub async fn cache_docker_path(uuid: Uuid, docker_path: String) -> Result<()> {
        Self::cache_docker_path_at(&machines_file_path()?, uuid, docker_path).await
    }

    async fn cache_docker_path_at(path: &Path, uuid: Uuid, docker_path: String) -> Result<()> {
        ensure!(
            !docker_path.is_empty() && !docker_path.contains('\0'),
            "The Docker executable path must be nonempty and contain no null character"
        );
        Self::mutate(path, move |store| {
            let existing = store
                .machines
                .iter_mut()
                .find(|current| current.uuid == uuid)
                .ok_or_else(|| anyhow!("No machine found with uuid '{uuid}'"))?;
            existing.docker_path = Some(docker_path);
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Remove a machine by UUID without overwriting unrelated concurrent edits.
    /// The last machine is retained so controllers always have a valid selection.
    pub async fn remove_machine(uuid: Uuid) -> Result<()> {
        Self::remove_at(&machines_file_path()?, uuid).await
    }

    async fn remove_at(path: &Path, uuid: Uuid) -> Result<()> {
        Self::mutate(path, move |store| {
            let index = store
                .machines
                .iter()
                .position(|machine| machine.uuid == uuid)
                .ok_or_else(|| anyhow!("No machine found with uuid '{uuid}'"))?;
            ensure!(store.machines.len() > 1, "Cannot remove the last machine");
            store.machines.remove(index);
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Append to the latest saved store, then refresh this snapshot after the
    /// atomic write succeeds. A stale pre-connection snapshot cannot erase edits.
    pub async fn create_machine(&mut self, machine: Machine) -> Result<usize> {
        self.create_at(&machines_file_path()?, machine).await
    }

    async fn create_at(&mut self, path: &Path, machine: Machine) -> Result<usize> {
        let (store, index) = Self::mutate(path, move |store| {
            ensure!(
                !store
                    .machines
                    .iter()
                    .any(|current| current.uuid == machine.uuid),
                "A machine with this identifier already exists"
            );
            let index = store.machines.len();
            store.machines.push(machine);
            Ok(index)
        })
        .await?;
        *self = store;
        Ok(index)
    }

    /// Connect first, then merge the new machine into the current saved store.
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
    fn default() -> Self {
        Self {
            machines: vec![Machine::default()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("crabdash-machines-test-{}", Uuid::new_v4())))
        }
        fn path(&self) -> PathBuf {
            self.0.join("machines.json")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn machine(id: &str) -> Machine {
        Machine {
            id: id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn concurrent_first_loads_share_the_same_nonempty_default() -> Result<()> {
        smol::block_on(async {
            let fixture = Fixture::new();
            let path = fixture.path();
            let (first, second) = smol::future::zip(
                MachineStore::load_from(&path),
                MachineStore::load_from(&path),
            )
            .await;
            let first = first?;
            let second = second?;
            assert_eq!(first.machines.len(), 1);
            assert_eq!(first.machines[0].uuid, second.machines[0].uuid);
            assert!(first.machines[0].remote.is_none());
            Ok(())
        })
    }

    #[test]
    fn concurrent_creation_merges_stale_snapshots_into_latest_store() -> Result<()> {
        smol::block_on(async {
            let fixture = Fixture::new();
            let path = fixture.path();
            let mut first = MachineStore::load_from(&path).await?;
            let mut second = first.clone();
            let a = machine("first remote");
            let b = machine("second remote");
            let expected = [first.machines[0].uuid, a.uuid, b.uuid];
            let (a, b) =
                smol::future::zip(first.create_at(&path, a), second.create_at(&path, b)).await;
            let a_index = a?;
            let b_index = b?;
            assert_eq!(first.machines[a_index].uuid, expected[1]);
            assert_eq!(second.machines[b_index].uuid, expected[2]);
            let saved = MachineStore::load_from(&path).await?;
            assert_eq!(saved.machines.len(), expected.len());
            for id in expected {
                assert!(saved.machines.iter().any(|machine| machine.uuid == id));
            }
            Ok(())
        })
    }

    #[test]
    fn stale_updates_and_creates_cannot_resurrect_a_deleted_machine() -> Result<()> {
        smol::block_on(async {
            let fixture = Fixture::new();
            let path = fixture.path();
            let mut store = MachineStore::load_from(&path).await?;
            let removed = machine("removed remote");
            store.create_at(&path, removed.clone()).await?;
            let mut stale = store.clone();
            MachineStore::remove_at(&path, removed.uuid).await?;
            let original = fs::read(&path).await?;
            assert!(
                MachineStore::cache_docker_path_at(&path, removed.uuid, "docker".into())
                    .await
                    .is_err()
            );
            assert!(
                MachineStore::update_system_info_at(
                    &path,
                    removed.uuid,
                    removed.system_info,
                    removed.kind
                )
                .await
                .is_err()
            );
            assert_eq!(fs::read(&path).await?, original);
            stale.create_at(&path, machine("new remote")).await?;
            let current = MachineStore::load_from(&path).await?;
            assert_eq!(current.machines.len(), 2);
            assert!(
                !current
                    .machines
                    .iter()
                    .any(|machine| machine.uuid == removed.uuid)
            );
            Ok(())
        })
    }

    #[test]
    fn last_machine_duplicate_identifiers_and_corrupt_files_are_preserved() -> Result<()> {
        smol::block_on(async {
            let fixture = Fixture::new();
            let path = fixture.path();
            let mut store = MachineStore::load_from(&path).await?;
            let original = fs::read(&path).await?;
            let original_id = store.machines[0].uuid;
            assert!(MachineStore::remove_at(&path, original_id).await.is_err());
            let duplicate = store.machines[0].clone();
            assert!(store.create_at(&path, duplicate).await.is_err());
            assert_eq!(fs::read(&path).await?, original);
            assert_eq!(store.machines.len(), 1);
            let corrupt = b"{broken machine data";
            fs::write(&path, corrupt).await?;
            assert!(store.create_at(&path, machine("new remote")).await.is_err());
            assert_eq!(fs::read(&path).await?, corrupt);
            assert_eq!(store.machines[0].uuid, original_id);
            fs::write(&path, br#"{"machines":[]}"#).await?;
            assert!(MachineStore::load_from(&path).await.is_err());
            let duplicate = MachineStore {
                machines: vec![store.machines[0].clone(), store.machines[0].clone()],
            };
            let duplicate = serde_json::to_vec(&duplicate)?;
            fs::write(&path, &duplicate).await?;
            assert!(MachineStore::load_from(&path).await.is_err());
            assert_eq!(fs::read(&path).await?, duplicate);
            Ok(())
        })
    }

    #[test]
    fn failed_atomic_replacement_preserves_target_and_cleans_temporary_file() -> Result<()> {
        smol::block_on(async {
            let fixture = Fixture::new();
            let path = fixture.path();
            let _guard = STORE_LOCK.lock().await;
            // A nonempty directory cannot be atomically replaced by a file on
            // any supported platform, providing a deterministic rename failure.
            fs::create_dir_all(&path).await?;
            let sentinel = path.join("preserved");
            fs::write(&sentinel, b"original data").await?;
            assert!(MachineStore::default().save_unlocked(&path).await.is_err());
            assert_eq!(fs::read(&sentinel).await?, b"original data");
            let entries = std::fs::read_dir(&fixture.0)?.collect::<std::io::Result<Vec<_>>>()?;
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].path(), path);
            Ok(())
        })
    }

    #[test]
    fn concurrent_identity_and_docker_cache_updates_preserve_independent_fields() -> Result<()> {
        smol::block_on(async {
            let fixture = Fixture::new();
            let path = fixture.path();
            let store = MachineStore::load_from(&path).await?;
            let uuid = store.machines[0].uuid;
            let info = SystemInfo {
                machine_name: "refreshed host".into(),
                os_version: "Linux 7.2".into(),
                arch: "aarch64".into(),
                distribution: None,
            };
            let cli_path = "/usr/local/bin/docker";
            let (identity, cache) = smol::future::zip(
                MachineStore::update_system_info_at(&path, uuid, info.clone(), MachineKind::Linux),
                MachineStore::cache_docker_path_at(&path, uuid, cli_path.into()),
            )
            .await;
            identity?;
            cache?;
            let current = MachineStore::load_from(&path).await?;
            assert_eq!(current.machines[0].system_info, info);
            assert!(matches!(current.machines[0].kind, MachineKind::Linux));
            assert_eq!(current.machines[0].docker_path.as_deref(), Some(cli_path));
            assert_eq!(current.machines[0].id, store.machines[0].id);
            Ok(())
        })
    }
}
