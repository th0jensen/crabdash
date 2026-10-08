use anyhow::{Result, anyhow, bail};
use async_ssh2_lite::{
    AsyncSession, TokioTcpStream,
    ssh2::{ExtendedData, KnownHostFileKind},
    tokio::io::{AsyncReadExt, AsyncWriteExt},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt::{Debug, Formatter},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::{
    runtime::Runtime,
    sync::{Mutex, MutexGuard},
};

use utils::{args::Args, output::Output};

static SSH_RT: OnceLock<std::io::Result<Runtime>> = OnceLock::new();

pub(crate) fn ssh_runtime() -> Result<&'static Runtime> {
    SSH_RT
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
        })
        .as_ref()
        .map_err(|error| anyhow!("Unable to initialize SSH runtime: {error}"))
}

#[derive(Serialize, Deserialize)]
pub struct RemoteConnection {
    pub user: String,
    pub host: String,
    pub auth: Option<AuthMethod>,
    #[serde(skip)]
    session: Arc<Mutex<Option<AsyncSession<TokioTcpStream>>>>,
    #[serde(skip)]
    connected: Arc<AtomicBool>,
}

impl RemoteConnection {
    /// Whether two snapshots share the same runtime SSH session owner. This
    /// remains stable across clones, while a replacement connection differs.
    pub fn shares_session_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.session, &other.session)
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn set_connected(&self, value: bool) {
        self.connected.store(value, Ordering::Relaxed);
    }
}

impl Default for RemoteConnection {
    fn default() -> Self {
        Self {
            user: String::new(),
            host: String::new(),
            auth: None,
            session: Arc::new(Mutex::new(None)),
            connected: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl RemoteConnection {
    pub async fn new_connection(
        user: impl Into<String>,
        host: impl Into<String>,
        auth: AuthMethod,
    ) -> Result<RemoteConnection> {
        let user = user.into();
        let host = host.into();
        tracing::debug!(user = %user, host = %host, "new_connection");
        let rc = RemoteConnection {
            user,
            host,
            auth: Some(auth),
            session: Arc::new(Mutex::new(None)),
            connected: Arc::new(AtomicBool::new(false)),
        };

        let sess = rc.connect().await?;
        *rc.session.lock().await = Some(sess);
        rc.set_connected(true);
        tracing::debug!("new_connection: success");
        Ok(rc)
    }

    pub async fn connect(&self) -> Result<AsyncSession<TokioTcpStream>> {
        let connection = self.clone();
        ssh_runtime()?
            .spawn(async move { connection.connect_inner().await })
            .await
            .map_err(|error| anyhow!("SSH task panicked: {error}"))
            .and_then(|result| result)
    }

    // Called directly on the SSH runtime by the bounded owning task. Never
    // spawn a nested task here: dropping a JoinHandle would detach connection
    // or authentication work after its sample had already timed out.
    async fn connect_inner(&self) -> Result<AsyncSession<TokioTcpStream>> {
        let tcp = TokioTcpStream::connect(format!("{}:22", self.host)).await?;
        let mut session = AsyncSession::new(tcp, None)?;
        session.handshake().await?;
        {
            let mut known_hosts = session.known_hosts()?;
            let path = dirs::home_dir()
                .ok_or_else(|| anyhow!("Could not locate the SSH home directory"))?
                .join(".ssh/known_hosts");
            if let Err(error) = known_hosts.read_file(&path, KnownHostFileKind::OpenSSH) {
                tracing::warn!(%error, path = %path.display(), "known_hosts read failed (non-fatal)");
            }
        }
        match self.auth.as_ref() {
            Some(AuthMethod::None) => session.userauth_password(&self.user, "").await?,
            Some(AuthMethod::AuthKey {
                pubkey,
                privatekey,
                passphrase,
            }) => {
                session
                    .userauth_pubkey_file(
                        &self.user,
                        pubkey.as_deref(),
                        privatekey,
                        passphrase.as_deref(),
                    )
                    .await?
            }
            Some(AuthMethod::Password(password)) => {
                session.userauth_password(&self.user, password).await?
            }
            None => session.userauth_agent(&self.user).await?,
        };
        if !session.authenticated() {
            bail!("Authentication failed!");
        }
        Ok(session)
    }

    pub async fn ensure_connected(&mut self) -> Result<()> {
        if self.session.lock().await.is_none() {
            self.set_connected(false);
            let sess = self.connect().await?;
            *self.session.lock().await = Some(sess);
            self.set_connected(true);
        }
        Ok(())
    }

    pub async fn run_ssh_command(&mut self, cmd: &str, args: &Args) -> Result<Output> {
        self.ensure_connected().await?;
        let result = ssh_runtime()?
            .spawn({
                let session = self.session.clone();
                let full_cmd = self.build_command(cmd, args);
                async move {
                    let session = session.lock().await;
                    let Some(session) = session.as_ref() else {
                        bail!("Not connected!");
                    };
                    execute(session, &full_cmd).await
                }
            })
            .await
            .map_err(|e| anyhow!("SSH task panicked: {e}"))
            .and_then(|result| result);

        if result.is_err() {
            self.session.lock().await.take();
            self.set_connected(false);
        }

        result
    }

    pub(crate) async fn run_ssh_command_until(
        &mut self,
        cmd: &str,
        args: &Args,
        deadline: Instant,
    ) -> Result<Output> {
        self.run_ssh_with_stdin_until(cmd, args, None, deadline)
            .await
    }

    pub(crate) async fn run_ssh_command_with_input_until(
        &mut self,
        cmd: &str,
        args: &Args,
        input: &[u8],
        deadline: Instant,
    ) -> Result<Output> {
        crate::command::check_deadline(deadline)?;
        self.run_ssh_with_stdin_until(cmd, args, Some(input.to_vec()), deadline)
            .await
    }

    async fn run_ssh_with_stdin_until(
        &mut self,
        cmd: &str,
        args: &Args,
        input: Option<Vec<u8>>,
        deadline: Instant,
    ) -> Result<Output> {
        crate::command::check_deadline(deadline)?;
        let connection = self.clone();
        let command = self.build_command(cmd, args);
        let task = ssh_runtime()?.spawn(async move {
            let session = connection.session.clone();
            let connected = connection.connected.clone();
            with_owned_session_until(&session, &connected, deadline, move |cached| {
                Box::pin(async move {
                    if cached.is_none() {
                        connection.set_connected(false);
                        *cached = Some(connection.connect_inner().await?);
                        connection.set_connected(true);
                    }
                    let session = cached.as_ref().ok_or_else(|| anyhow!("Not connected!"))?;
                    execute_with_input(session, &command, input.as_deref()).await
                })
            })
            .await
        });
        crate::command::AbortOnDrop::new(task)
            .await
            .map_err(|error| anyhow!("SSH task failed: {error}"))?
    }

    fn build_command(&self, cmd: &str, args: &Args) -> String {
        let shell_quote = |s: &String| -> String { format!("'{}'", s.replace('\'', "'\\''")) };
        let quoted_args: String = args.iter().map(shell_quote).collect::<Vec<_>>().join(" ");
        if quoted_args.is_empty() {
            cmd.to_string()
        } else {
            format!("{} {}", cmd, quoted_args)
        }
    }

    pub async fn has_active_session(&self) -> bool {
        self.session
            .lock()
            .await
            .as_ref()
            .map_or(false, |session| session.authenticated())
    }

    pub fn restore_session_from(&mut self, other: &RemoteConnection) {
        self.session = Arc::clone(&other.session);
        self.connected = Arc::clone(&other.connected);
    }
}

async fn execute(session: &AsyncSession<TokioTcpStream>, command: &str) -> Result<Output> {
    execute_with_input(session, command, None).await
}

async fn execute_with_input(
    session: &AsyncSession<TokioTcpStream>,
    command: &str,
    input: Option<&[u8]>,
) -> Result<Output> {
    let mut channel = session.channel_session().await?;
    channel.handle_extended_data(ExtendedData::Merge).await?;
    channel.exec(command).await?;
    let mut output = Vec::new();
    let written = if let Some(input) = input {
        // An independent stream handle allows output to drain while channel
        // input is backpressured. EOF finishes only stdin, not the session.
        let mut reader = channel.stream(0);
        let (finished, closed) = tokio::sync::oneshot::channel();
        let (read, written) = tokio::join!(
            async {
                let result = reader.read_to_end(&mut output).await;
                let _ = finished.send(());
                result
            },
            async {
                // A peer can exit without consuming stdin. Its output EOF
                // must stop a blocked writer so its diagnostic remains visible.
                let written = tokio::select! {
                    biased;
                    result = channel.write_all(input) => result,
                    _ = closed => Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe, "SSH peer closed command output before accepting its input"
                    )),
                };
                // ssh2::Stream::flush discards received data; it is not a
                // send-buffer flush. EOF also completes a rejected command.
                let eof = channel.send_eof().await;
                written?;
                eof?;
                Ok::<_, anyhow::Error>(())
            }
        );
        read?;
        written
    } else {
        channel.read_to_end(&mut output).await?;
        Ok(())
    };
    channel.wait_close().await?;
    let exit_status = channel.exit_status()?;
    if exit_status != 0 {
        let message = String::from_utf8_lossy(&output).trim().to_string();
        if message.is_empty() {
            bail!("{command} failed with exit status: {exit_status}");
        }
        bail!("{command} failed with exit status {exit_status}: {message}");
    }
    // A rejected command can close stdin early; retain its status and stderr
    // before reporting a transport write failure from an otherwise successful command.
    written?;
    Ok(Output::from(output))
}

/// Own cleanup while the lock is still held. In particular, cancellation must
/// not release this guard and then reacquire a possibly unrelated new session.
struct OwnedSession<'a, T> {
    guard: MutexGuard<'a, Option<T>>,
    connected: &'a AtomicBool,
    keep: bool,
}
impl<T> Drop for OwnedSession<'_, T> {
    fn drop(&mut self) {
        if !self.keep {
            self.guard.take();
            self.connected.store(false, Ordering::Relaxed);
        }
    }
}

async fn with_owned_session_until<T: Send, R>(
    session: &Mutex<Option<T>>,
    connected: &AtomicBool,
    deadline: Instant,
    operation: impl for<'a> FnOnce(
        &'a mut Option<T>,
    ) -> Pin<Box<dyn Future<Output = Result<R>> + Send + 'a>>,
) -> Result<R> {
    crate::command::check_deadline(deadline)?;
    let timer_deadline = tokio::time::Instant::from_std(deadline);
    // Before acquisition we own nothing. A queue timeout must not invalidate
    // the healthy session or connected flag belonging to the current owner.
    let guard = tokio::time::timeout_at(timer_deadline, session.lock())
        .await
        .map_err(|_| anyhow!("SSH command deadline exceeded while waiting for the session"))?;
    // A ready lock can race an already-ready timer. Do not start new work or
    // invalidate a healthy cached session when the budget was spent queuing.
    crate::command::check_deadline(deadline)?;
    let mut owned = OwnedSession {
        guard,
        connected,
        keep: false,
    };
    let result = tokio::time::timeout_at(timer_deadline, operation(&mut owned.guard))
        .await
        .map_err(|_| anyhow!("SSH command deadline exceeded"))?;
    owned.keep = result.is_ok();
    result
}

impl Clone for RemoteConnection {
    fn clone(&self) -> Self {
        Self {
            user: self.user.clone(),
            host: self.host.clone(),
            auth: self.auth.clone(),
            session: Arc::clone(&self.session),
            connected: Arc::clone(&self.connected),
        }
    }
}

impl Debug for RemoteConnection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RemoteConnection {{ user: {}, host: {} }}",
            self.user, self.host
        )
    }
}

#[derive(Clone)]
pub enum AuthMethod {
    None,
    Password(String),
    AuthKey {
        pubkey: Option<PathBuf>,
        privatekey: PathBuf,
        passphrase: Option<String>,
    },
}

impl AuthMethod {
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Password(_) => "password",
            Self::AuthKey { .. } => "pubkey",
        }
    }

    pub fn secret_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::None => None,
            Self::Password(password) if !password.trim().is_empty() => {
                Some(password.as_bytes().to_vec())
            }
            Self::AuthKey {
                passphrase: Some(passphrase),
                ..
            } if !passphrase.trim().is_empty() => Some(passphrase.as_bytes().to_vec()),
            _ => None,
        }
    }

    pub fn apply_secret(&mut self, secret: String) -> () {
        match self {
            Self::None => {}
            Self::Password(password) => *password = secret,
            Self::AuthKey { passphrase, .. } => *passphrase = Some(secret),
        }
    }
}

impl Serialize for AuthMethod {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let def = match self {
            AuthMethod::None => AuthMethodDef::None,
            AuthMethod::Password(_) => AuthMethodDef::Password,
            AuthMethod::AuthKey {
                pubkey, privatekey, ..
            } => AuthMethodDef::AuthKey {
                pubkey: pubkey.clone(),
                privatekey: privatekey.clone(),
            },
        };
        def.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AuthMethod {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match AuthMethodDef::deserialize(deserializer)? {
            AuthMethodDef::Password => AuthMethod::Password(String::new()),
            AuthMethodDef::AuthKey { pubkey, privatekey } => AuthMethod::AuthKey {
                pubkey,
                privatekey,
                passphrase: None,
            },
            AuthMethodDef::None => AuthMethod::None,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AuthMethodDef {
    None,
    Password,
    AuthKey {
        pubkey: Option<PathBuf>,
        privatekey: PathBuf,
    },
}

#[cfg(test)]
mod deadline_tests {
    use super::*;
    use std::time::Duration;
    #[cfg(target_os = "linux")]
    use utils::args;

    #[tokio::test]
    async fn owned_timeout_clears_session_unlocks_and_allows_subsequent_success() -> Result<()> {
        let session = Mutex::new(Some(7_u32));
        let connected = Arc::new(AtomicBool::new(true));
        let result: Result<()> = with_owned_session_until(
            &session,
            &connected,
            Instant::now() + Duration::from_millis(30),
            |cached| {
                Box::pin(async move {
                    assert_eq!(*cached, Some(7));
                    std::future::pending().await
                })
            },
        )
        .await;
        assert!(result.is_err());
        assert!(session.lock().await.is_none());
        assert!(!connected.load(Ordering::Relaxed));
        let flag = connected.clone();
        let value = with_owned_session_until(
            &session,
            &connected,
            Instant::now() + Duration::from_secs(1),
            move |cached| {
                Box::pin(async move {
                    assert!(cached.is_none());
                    *cached = Some(11);
                    flag.store(true, Ordering::Relaxed);
                    Ok(42)
                })
            },
        )
        .await?;
        assert_eq!(value, 42);
        assert_eq!(*session.lock().await, Some(11));
        assert!(connected.load(Ordering::Relaxed));
        Ok(())
    }

    #[tokio::test]
    async fn lock_wait_timeout_leaves_the_current_owner_untouched() -> Result<()> {
        let session = Mutex::new(Some(19_u32));
        let connected = AtomicBool::new(true);
        let owner = session.lock().await;
        let invoked = Arc::new(AtomicBool::new(false));
        let marker = invoked.clone();
        let result = with_owned_session_until(
            &session,
            &connected,
            Instant::now() + Duration::from_millis(30),
            move |_| {
                Box::pin(async move {
                    marker.store(true, Ordering::Relaxed);
                    Ok(())
                })
            },
        )
        .await;
        assert!(result.is_err());
        assert!(!invoked.load(Ordering::Relaxed));
        assert_eq!(*owner, Some(19));
        assert!(connected.load(Ordering::Relaxed));
        drop(owner);
        let value = with_owned_session_until(
            &session,
            &connected,
            Instant::now() + Duration::from_secs(1),
            |cached| Box::pin(async move { Ok(*cached) }),
        )
        .await?;
        assert_eq!(value, Some(19));
        Ok(())
    }

    #[tokio::test]
    async fn cancelling_the_task_aborts_the_owner_and_releases_its_session() -> Result<()> {
        let session = Arc::new(Mutex::new(Some(31_u32)));
        let connected = Arc::new(AtomicBool::new(true));
        let acquired = Arc::new(tokio::sync::Notify::new());
        let task = {
            let session = session.clone();
            let connected = connected.clone();
            let acquired = acquired.clone();
            crate::command::AbortOnDrop::new(tokio::spawn(async move {
                with_owned_session_until(
                    &session,
                    &connected,
                    Instant::now() + Duration::from_secs(10),
                    move |_| {
                        Box::pin(async move {
                            acquired.notify_one();
                            std::future::pending::<Result<()>>().await
                        })
                    },
                )
                .await
            }))
        };
        tokio::time::timeout(Duration::from_secs(1), acquired.notified()).await?;
        drop(task);
        let guard = tokio::time::timeout(Duration::from_secs(1), session.lock()).await?;
        assert!(guard.is_none());
        assert!(!connected.load(Ordering::Relaxed));
        drop(guard);
        with_owned_session_until(
            &session,
            &connected,
            Instant::now() + Duration::from_secs(1),
            |cached| {
                Box::pin(async move {
                    *cached = Some(43);
                    Ok(())
                })
            },
        )
        .await?;
        assert_eq!(*session.lock().await, Some(43));
        Ok(())
    }

    #[tokio::test]
    async fn expired_ssh_budget_does_not_connect_or_touch_another_owner() -> Result<()> {
        let mut connection = RemoteConnection::default();
        connection.set_connected(true);
        let session = connection.session.clone();
        let _owner = session.lock().await;
        assert!(
            connection
                .run_ssh_command_until("printf never", &Args::new(), Instant::now())
                .await
                .is_err()
        );
        assert!(connection.connected());
        Ok(())
    }

    /// Root runs this binary in an isolated network namespace with a temporary
    /// sshd, keys and authorized_keys. It never uses normal account credentials.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires isolated loopback sshd and explicit CRABDASH_SSH_DEADLINE_TEST_KEY"]
    async fn authenticated_ssh_timeout_recovers_and_lock_wait_preserves_the_owner() -> Result<()> {
        let privatekey = PathBuf::from(std::env::var("CRABDASH_SSH_DEADLINE_TEST_KEY")?);
        let pubkey = std::env::var_os("CRABDASH_SSH_DEADLINE_TEST_PUBKEY").map(PathBuf::from);
        let mut connection = tokio::time::timeout(
            Duration::from_secs(5),
            RemoteConnection::new_connection(
                "thomas",
                "127.0.0.1",
                AuthMethod::AuthKey {
                    pubkey,
                    privatekey,
                    passphrase: None,
                },
            ),
        )
        .await??;
        // Input exceeds both Windows command-line limits and common pipe/SSH
        // windows. The peer writes before reading, so delivery must drain
        // merged stdout/stderr concurrently and finish stdin with EOF.
        let input = "雪 ‘stdin’\n".repeat(600_000).into_bytes();
        let command = "python3 -c \"import sys; sys.stdout.buffer.write(b'o'*4194304); sys.stdout.flush(); sys.stderr.buffer.write(b'e'*4194304); sys.stderr.flush(); sys.stdout.buffer.write(sys.stdin.buffer.read())\"";
        let output = connection
            .run_ssh_command_with_input_until(
                command,
                &Args::new(),
                &input,
                Instant::now() + Duration::from_secs(15),
            )
            .await
            .map_err(|error| anyhow!("Large SSH input/output delivery: {error:#}"))?;
        let output = output.as_ref();
        assert_eq!(output.len(), 8_388_608 + input.len());
        assert_eq!(&output[8_388_608..], input);
        assert!(
            output[..8_388_608]
                .iter()
                .all(|byte| matches!(byte, b'o' | b'e'))
        );
        assert_eq!(
            connection
                .run_ssh_command_with_input_until(
                    "cat; printf eof",
                    &Args::new(),
                    &[],
                    Instant::now() + Duration::from_secs(2),
                )
                .await?
                .as_ref(),
            b"eof",
        );
        let rejected = connection
            .run_ssh_command_with_input_until(
                "printf denied >&2; exit 7",
                &Args::new(),
                &vec![b'i'; 8 * 1024 * 1024],
                Instant::now() + Duration::from_secs(2),
            )
            .await;
        let diagnostic = rejected
            .err()
            .ok_or_else(|| anyhow!("Expected rejection"))?;
        assert!(diagnostic.to_string().contains("denied"), "{diagnostic:#}");
        assert!(
            diagnostic.to_string().contains("exit status 7"),
            "{diagnostic:#}"
        );
        let blocked = connection
            .run_ssh_command_with_input_until(
                "exec sleep 10",
                &Args::new(),
                &input,
                Instant::now() + Duration::from_millis(300),
            )
            .await;
        assert!(blocked.is_err());
        assert!(!connection.connected());
        assert!(connection.session.lock().await.is_none());
        let timed_out = connection
            .run_ssh_command_until(
                "exec sleep 10",
                &Args::new(),
                Instant::now() + Duration::from_millis(300),
            )
            .await;
        let timeout_error = timed_out
            .err()
            .ok_or_else(|| anyhow!("Expected the sleeping SSH command to time out"))?;
        assert!(
            timeout_error.to_string().contains("deadline exceeded"),
            "Expected a command deadline failure, received: {timeout_error:#}"
        );
        assert!(!connection.connected());
        assert!(connection.session.lock().await.is_none());
        let recovered = connection
            .run_ssh_command_until(
                "printf recovered",
                &Args::new(),
                Instant::now() + Duration::from_secs(5),
            )
            .await?;
        assert_eq!(recovered.as_ref(), b"recovered");
        assert!(connection.connected());
        let mut owner = connection.clone();
        let owner = crate::command::AbortOnDrop::new(tokio::spawn(async move {
            owner
                .run_ssh_command_until(
                    "sleep 1; printf owner",
                    &Args::new(),
                    Instant::now() + Duration::from_secs(5),
                )
                .await
        }));
        let until = Instant::now() + Duration::from_secs(2);
        while connection.session.try_lock().is_ok() {
            crate::command::check_deadline(until)?;
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let waiter = connection
            .run_ssh_command_until(
                "printf never",
                &args![],
                Instant::now() + Duration::from_millis(100),
            )
            .await;
        let waiter_error = waiter
            .err()
            .ok_or_else(|| anyhow!("Expected the SSH session waiter to time out"))?;
        assert!(
            waiter_error
                .to_string()
                .contains("deadline exceeded while waiting for the session"),
            "Expected a session queue deadline failure, received: {waiter_error:#}"
        );
        assert!(connection.connected());
        assert!(connection.session.try_lock().is_err());
        assert_eq!(owner.await??.as_ref(), b"owner");
        assert!(connection.connected());
        assert!(connection.session.lock().await.is_some());
        let final_read = connection
            .run_ssh_command_until(
                "printf final",
                &Args::new(),
                Instant::now() + Duration::from_secs(2),
            )
            .await?;
        assert_eq!(final_read.as_ref(), b"final");
        Ok(())
    }
}
