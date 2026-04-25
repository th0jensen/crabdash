use anyhow::{Result, anyhow, bail};
use async_ssh2_lite::{
    AsyncSession, TokioTcpStream, ssh2::KnownHostFileKind, tokio::io::AsyncReadExt,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    env::var,
    fmt::{Debug, Formatter},
    path::Path,
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{runtime::Runtime, sync::Mutex};

use utils::{args::Args, output::Output};

static SSH_RT: OnceLock<Runtime> = OnceLock::new();

fn ssh_rt() -> &'static Runtime {
    SSH_RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Failed to create SSH runtime")
    })
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
        eprintln!("[SSH] new_connection: user={user} host={host}");
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
        eprintln!("[SSH] new_connection: success");
        Ok(rc)
    }

    pub async fn connect(&self) -> Result<AsyncSession<TokioTcpStream>> {
        let host = self.host.clone();
        let user = self.user.clone();
        let auth = self.auth.clone();

        ssh_rt()
            .spawn(async move {
                let tcp = match TokioTcpStream::connect(format!("{host}:22")).await {
                    Ok(s) => {
                        eprintln!("[SSH] connect: TCP ok");
                        s
                    }
                    Err(e) => {
                        eprintln!("[SSH] connect: TCP failed: {e}");
                        return Err(e.into());
                    }
                };

                let mut sess = AsyncSession::new(tcp, None)?;
                sess.handshake().await?;

                {
                    let mut known_hosts = sess.known_hosts()?;
                    let kh_path = format!("{}/.ssh/known_hosts", var("HOME")?);
                    if let Err(e) =
                        known_hosts.read_file(Path::new(&kh_path), KnownHostFileKind::OpenSSH)
                    {
                        eprintln!("[SSH] connect: known_hosts read failed (non-fatal): {e}");
                    }
                }

                match &auth {
                    Some(AuthMethod::None) => sess.userauth_password(&user, "").await?,
                    Some(AuthMethod::AuthKey {
                        pubkey,
                        privatekey,
                        passphrase,
                    }) => {
                        sess.userauth_pubkey_file(
                            &user,
                            pubkey.as_deref(),
                            privatekey,
                            passphrase.as_deref(),
                        )
                        .await?
                    }
                    Some(AuthMethod::Password(password)) => {
                        sess.userauth_password(&user, password).await?
                    }
                    None => sess.userauth_agent(&user).await?,
                };

                if !sess.authenticated() {
                    bail!("Authentication failed!");
                }
                Ok(sess)
            })
            .await
            .map_err(|e| anyhow!("SSH task panicked: {e}"))
            .and_then(|r| r)
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
        ssh_rt()
            .spawn({
                let session = self.session.clone();
                let full_cmd = self.build_command(cmd, args);
                async move {
                    let session = session.lock().await;
                    let Some(ref session) = *session else {
                        bail!("Not connected!");
                    };
                    let mut channel = session.channel_session().await?;
                    channel.exec(&full_cmd).await?;
                    let mut stdout = Vec::new();
                    channel.read_to_end(&mut stdout).await?;
                    channel.wait_close().await?;
                    let exit_status = channel.exit_status()?;
                    if exit_status != 0 {
                        bail!("{full_cmd} failed with exit status: {exit_status}");
                    }
                    Ok(Output::from(stdout))
                }
            })
            .await
            .map_err(|e| anyhow!("SSH task panicked: {e}"))
            .and_then(|r| r)
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
