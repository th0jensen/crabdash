//! One desktop owner per OS user. The permanent lock file is never unlinked.
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use smol::channel::{Receiver, Sender};
use std::{
    fs::{self, File, TryLockError},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use uuid::Uuid;

const STARTUP_WAIT: Duration = Duration::from_secs(3);
const IO_WAIT: Duration = Duration::from_millis(350);
const MAX_FRAME: usize = 4096;

pub(super) enum Claim {
    Primary(Instance),
    Forwarded,
}

pub(super) struct Instance {
    // Held until the server has stopped and its endpoint has been removed.
    _lock: File,
    endpoint_path: PathBuf,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    pub(super) requests: Receiver<Option<String>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    port: u16,
    nonce: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Show {
    nonce: String,
    token: Option<String>,
}

pub(super) fn claim() -> Result<Claim> {
    let directory = platform::directory()?;
    let token = std::env::var("XDG_ACTIVATION_TOKEN")
        .ok()
        .filter(|value| !value.is_empty());
    claim_at(&directory, token, STARTUP_WAIT)
}

fn claim_at(directory: &Path, token: Option<String>, wait: Duration) -> Result<Claim> {
    platform::prepare_directory(directory)?;
    let lock_path = directory.join("instance.lock");
    reject_symlink(&lock_path)?;
    let mut options = platform::file_options();
    let lock = options
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .context("Unable to open the Crabdash instance lock")?;
    platform::validate_file(&lock)?;
    let endpoint_path = directory.join("endpoint.json");
    let deadline = Instant::now() + wait;
    loop {
        match lock.try_lock() {
            Ok(()) => return start_owner(lock, endpoint_path).map(Claim::Primary),
            Err(TryLockError::WouldBlock) => {
                if read_endpoint(&endpoint_path)
                    .and_then(|endpoint| forward(&endpoint, token.clone()))
                    .is_ok()
                {
                    return Ok(Claim::Forwarded);
                }
                // Recheck the OS lock after each failed handshake. A crashed
                // owner may leave metadata, but never retains its kernel lock.
                if Instant::now() >= deadline {
                    bail!(
                        "Crabdash is already running, but its window could not be reached. Quit the existing Crabdash process and try again."
                    );
                }
                thread::sleep(Duration::from_millis(40));
            }
            Err(TryLockError::Error(error)) => {
                return Err(error).context("Unable to lock the Crabdash instance");
            }
        }
    }
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => ensure!(
            !metadata.file_type().is_symlink() && metadata.is_file(),
            "Invalid Crabdash instance file"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("Unable to inspect the Crabdash instance file"),
    }
    Ok(())
}

fn read_endpoint(path: &Path) -> Result<Endpoint> {
    reject_symlink(path)?;
    let mut options = platform::file_options();
    let file = options.read(true).open(path)?;
    platform::validate_file(&file)?;
    let mut bytes = Vec::new();
    file.take(1025).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024, "Invalid Crabdash instance endpoint");
    let endpoint: Endpoint = serde_json::from_slice(&bytes)?;
    ensure!(
        endpoint.port != 0 && Uuid::parse_str(&endpoint.nonce).is_ok(),
        "Invalid Crabdash instance endpoint"
    );
    Ok(endpoint)
}

fn publish_endpoint(path: &Path, endpoint: &Endpoint) -> Result<()> {
    reject_symlink(path)?;
    let temporary = path.with_file_name(format!("endpoint-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut options = platform::file_options();
        let mut file = options.write(true).create_new(true).open(&temporary)?;
        file.write_all(&serde_json::to_vec(endpoint)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn start_owner(lock: File, endpoint_path: PathBuf) -> Result<Instance> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .context("Unable to start Crabdash instance communication")?;
    listener.set_nonblocking(true)?;
    let endpoint = Endpoint {
        port: listener.local_addr()?.port(),
        nonce: Uuid::new_v4().to_string(),
    };
    let (sender, requests) = smol::channel::bounded(16);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let nonce = endpoint.nonce.clone();
    let worker = thread::Builder::new()
        .name("crabdash-instance".into())
        .spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, address)) if address.ip().is_loopback() => {
                        let accepted = receive_show(&mut stream, &nonce, &sender).is_ok();
                        let _ = stream.set_write_timeout(Some(IO_WAIT));
                        let _ = stream.write_all(if accepted { b"OK\n" } else { b"ERR\n" });
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20))
                    }
                    Err(_) => break,
                }
            }
        })
        .context("Unable to start the Crabdash instance listener")?;
    let instance = Instance {
        _lock: lock,
        endpoint_path,
        stop,
        worker: Some(worker),
        requests,
    };
    publish_endpoint(&instance.endpoint_path, &endpoint)
        .context("Unable to publish the Crabdash instance endpoint")?;
    Ok(instance)
}

fn read_frame(stream: &mut TcpStream, maximum: usize) -> Result<Vec<u8>> {
    let deadline = Instant::now() + IO_WAIT;
    let mut frame = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(!remaining.is_zero(), "Crabdash instance request timed out");
        stream.set_read_timeout(Some(remaining))?;
        let mut chunk = [0; 256];
        let count = stream.read(&mut chunk)?;
        ensure!(count > 0, "Incomplete Crabdash instance request");
        frame.extend_from_slice(&chunk[..count]);
        ensure!(
            frame.len() <= maximum,
            "Crabdash instance request is too large"
        );
        if let Some(end) = frame.iter().position(|byte| *byte == b'\n') {
            ensure!(end + 1 == frame.len(), "Invalid Crabdash instance frame");
            frame.truncate(end);
            return Ok(frame);
        }
    }
}

fn receive_show(
    stream: &mut TcpStream,
    nonce: &str,
    sender: &Sender<Option<String>>,
) -> Result<()> {
    let request: Show = serde_json::from_slice(&read_frame(stream, MAX_FRAME)?)?;
    ensure!(
        request.nonce.len() == nonce.len()
            && request
                .nonce
                .bytes()
                .zip(nonce.bytes())
                .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
                == 0,
        "Invalid Crabdash instance authentication"
    );
    ensure!(
        request
            .token
            .as_ref()
            .is_none_or(|token| token.len() <= 2048 && !token.contains('\0')),
        "Invalid desktop activation token"
    );
    sender
        .try_send(request.token)
        .context("Crabdash is not ready to show its window")?;
    Ok(())
}

fn forward(endpoint: &Endpoint, token: Option<String>) -> Result<()> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port));
    let mut stream = TcpStream::connect_timeout(&address, IO_WAIT)?;
    stream.set_write_timeout(Some(IO_WAIT))?;
    let mut frame = serde_json::to_vec(&Show {
        nonce: endpoint.nonce.clone(),
        token,
    })?;
    frame.push(b'\n');
    ensure!(
        frame.len() <= MAX_FRAME,
        "Crabdash instance request is too large"
    );
    stream.write_all(&frame)?;
    ensure!(
        read_frame(&mut stream, 16)? == b"OK",
        "The existing Crabdash instance rejected the request"
    );
    Ok(())
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // Cleanup occurs while the permanent file is still locked. Another
        // owner can only publish its metadata after this guard has dropped.
        let _ = fs::remove_file(&self.endpoint_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        prelude::v1::test,
        process::{Child, Command, Stdio},
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Result<Self> {
            let path =
                std::env::temp_dir().join(format!("crabdash-instance-test-{}", Uuid::new_v4()));
            platform::prepare_directory(&path)?;
            Ok(Self(path))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    struct OwnerProcess(Child);
    impl Drop for OwnerProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn owner(directory: &Path) -> Result<OwnerProcess> {
        let child = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "desktop::instance::tests::subprocess_owner",
                "--ignored",
            ])
            .env("CRABDASH_TEST_INSTANCE_DIRECTORY", directory)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let owner = OwnerProcess(child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while read_endpoint(&directory.join("endpoint.json")).is_err() {
            ensure!(Instant::now() < deadline, "Test owner did not start");
            thread::sleep(Duration::from_millis(20));
        }
        Ok(owner)
    }

    #[test]
    #[ignore = "Subprocess helper; launched by instance integration tests"]
    fn subprocess_owner() -> Result<()> {
        let Some(directory) = std::env::var_os("CRABDASH_TEST_INSTANCE_DIRECTORY") else {
            return Ok(());
        };
        let Claim::Primary(_owner) = claim_at(Path::new(&directory), None, STARTUP_WAIT)? else {
            bail!("Expected primary test owner");
        };
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }

    #[test]
    fn separate_process_contention_forwards_and_crash_releases_the_same_lock() -> Result<()> {
        let directory = Fixture::new()?;
        let mut owner = owner(&directory.0)?;
        let old = read_endpoint(&directory.0.join("endpoint.json"))?;
        for _ in 0..4 {
            assert!(matches!(
                claim_at(&directory.0, None, STARTUP_WAIT)?,
                Claim::Forwarded
            ));
        }
        owner.0.kill()?;
        owner.0.wait()?;
        let Claim::Primary(_new_owner) = claim_at(&directory.0, None, STARTUP_WAIT)? else {
            bail!("Crash must release the instance lock");
        };
        assert_ne!(
            read_endpoint(&directory.0.join("endpoint.json"))?.nonce,
            old.nonce
        );
        assert!(directory.0.join("instance.lock").is_file());
        Ok(())
    }

    #[test]
    fn invalid_auth_and_oversized_requests_cannot_enqueue_show() -> Result<()> {
        let directory = Fixture::new()?;
        let Claim::Primary(owner) = claim_at(&directory.0, None, STARTUP_WAIT)? else {
            bail!("Expected primary");
        };
        let endpoint = read_endpoint(&directory.0.join("endpoint.json"))?;
        let bad = Endpoint {
            port: endpoint.port,
            nonce: Uuid::new_v4().to_string(),
        };
        assert!(forward(&bad, None).is_err());
        assert!(owner.requests.try_recv().is_err());
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, endpoint.port))?;
        stream.write_all(&vec![b'A'; MAX_FRAME + 1])?;
        assert_ne!(read_frame(&mut stream, 16).ok(), Some(b"OK".to_vec()));
        assert!(owner.requests.try_recv().is_err());
        forward(&endpoint, Some("desktop-token".into()))?;
        assert_eq!(owner.requests.try_recv()?, Some("desktop-token".into()));
        Ok(())
    }

    #[test]
    fn locked_unreachable_owner_fails_closed_and_unlocked_stale_endpoint_recovers() -> Result<()> {
        let directory = Fixture::new()?;
        let mut options = platform::file_options();
        let lock = options
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.0.join("instance.lock"))?;
        lock.lock()?;
        #[cfg(unix)]
        let lock_inode = {
            use std::os::unix::fs::MetadataExt as _;
            lock.metadata()?.ino()
        };
        let endpoint_path = directory.0.join("endpoint.json");
        let mut options = platform::file_options();
        let mut stale = options.write(true).create_new(true).open(&endpoint_path)?;
        stale.write_all(b"{malformed stale endpoint from a crashed process")?;
        drop(stale);
        assert!(claim_at(&directory.0, None, Duration::from_millis(100)).is_err());
        drop(lock);
        let Claim::Primary(_owner) = claim_at(&directory.0, None, STARTUP_WAIT)? else {
            bail!("Unlocked stale endpoint must allow a new owner");
        };
        let replaced = read_endpoint(&endpoint_path)?;
        assert!(replaced.port > 0);
        assert!(Uuid::parse_str(&replaced.nonce).is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            assert_eq!(
                fs::metadata(directory.0.join("instance.lock"))?.ino(),
                lock_inode
            );
        }
        Ok(())
    }
}
