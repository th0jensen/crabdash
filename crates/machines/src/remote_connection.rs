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
    sync::{Arc, Mutex, OnceLock},
};
use tokio::runtime::Runtime;

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
}

impl Default for RemoteConnection {
    fn default() -> Self {
        Self {
            user: String::new(),
            host: String::new(),
            auth: None,
            session: Arc::new(Mutex::new(None)),
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
        };

        let sess = rc.connect().await?;
        *rc.session.lock().unwrap() = Some(sess);
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
        if self.session.lock().unwrap().is_none() {
            let sess = self.connect().await?;
            *self.session.lock().unwrap() = Some(sess);
        }
        Ok(())
    }

    pub async fn run_ssh_command(
        &mut self,
        cmd: &str,
        args: Option<&[&str]>,
    ) -> Result<(String, i32)> {
        self.ensure_connected().await?;
        let session = self.session.lock().unwrap().clone();
        let Some(session) = session else {
            bail!("Not connected!")
        };

        let full_cmd = match args {
            Some(args) if !args.is_empty() => {
                let escaped_args = args
                    .iter()
                    .map(|arg| Self::shell_escape(arg))
                    .collect::<Vec<_>>()
                    .join(" ");

                format!("{cmd} {escaped_args}")
            }
            _ => cmd.to_string(),
        };

        ssh_rt()
            .spawn(async move {
                let mut channel = session.channel_session().await?;
                channel.exec(&full_cmd).await?;
                let mut s = String::new();
                channel.read_to_string(&mut s).await?;
                channel.wait_close().await?;
                let exit_status = channel.exit_status()?;
                Ok((s, exit_status))
            })
            .await
            .map_err(|e| anyhow!("SSH task panicked: {e}"))
            .and_then(|r| r)
    }

    pub fn has_active_session(&self) -> bool {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .map_or(false, |session| session.authenticated())
    }

    fn shell_escape(arg: &str) -> String {
        if arg.is_empty() {
            return "''".to_string();
        }

        if arg.bytes().all(|b| {
            matches!(
                b,
                b'a'..=b'z'
                    | b'A'..=b'Z'
                    | b'0'..=b'9'
                    | b'-' | b'_' | b'.' | b'/' | b':'
            )
        }) {
            return arg.to_string();
        }

        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

impl Clone for RemoteConnection {
    fn clone(&self) -> Self {
        Self {
            user: self.user.clone(),
            host: self.host.clone(),
            auth: self.auth.clone(),
            session: Arc::clone(&self.session),
        }
    }
}

impl Debug for RemoteConnection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RemoteConnection {{ connected: {} }}",
            self.session
                .lock()
                .unwrap()
                .as_ref()
                .map_or(false, |s| s.authenticated())
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
