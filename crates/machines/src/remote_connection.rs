use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ssh2::Session;
use std::{
    fmt::{Debug, Formatter},
    io::Read,
    net::TcpStream,
    path::PathBuf,
};

#[derive(Default, Serialize, Deserialize)]
pub struct RemoteConnection {
    pub user: String,
    pub host: String,
    pub auth: Option<AuthMethod>,
    #[serde(skip)]
    session: Option<Session>,
}

impl RemoteConnection {
    pub fn new_connection(
        user: impl Into<String>,
        host: impl Into<String>,
        auth: AuthMethod,
    ) -> Result<RemoteConnection> {
        let user = user.into();
        let host = host.into();
        let mut rc = RemoteConnection {
            user,
            host,
            auth: Some(auth),
            session: None,
        };

        let sess = rc.connect()?;
        rc.session = Some(sess);
        Ok(rc)
    }

    pub fn connect(&self) -> Result<Session> {
        let host = &self.host;
        let tcp = TcpStream::connect(format!("{host}:22"))?;
        let mut sess = Session::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;

        let mut known_hosts = sess.known_hosts()?;
        known_hosts.read_file(
            &std::path::Path::new(&format!("{}/.ssh/known_hosts", std::env::var("HOME")?)),
            ssh2::KnownHostFileKind::OpenSSH,
        )?;

        match &self.auth {
            Some(AuthMethod::None) => sess.userauth_password(&self.user, "")?,
            Some(AuthMethod::AuthKey {
                pubkey,
                privatekey,
                passphrase,
            }) => sess.userauth_pubkey_file(
                &self.user,
                pubkey.as_deref(),
                &privatekey,
                passphrase.as_deref(),
            )?,
            Some(AuthMethod::Password(password)) => sess.userauth_password(&self.user, password)?,
            None => sess.userauth_agent(&self.user)?,
        };

        if !sess.authenticated() {
            bail!("Authentication failed!");
        }
        Ok(sess)
    }

    pub fn ensure_connected(&mut self) -> Result<&Session> {
        if self.session.is_none() {
            self.session = Some(self.connect()?);
        }
        Ok(self.session.as_ref().unwrap())
    }

    pub fn run_ssh_command(&mut self, cmd: &str, args: Option<&[&str]>) -> Result<(String, i32)> {
        let session = self.ensure_connected()?;
        let mut channel = session.channel_session()?;

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

        channel.exec(&full_cmd)?;
        let mut s = String::new();
        channel.read_to_string(&mut s)?;
        channel.wait_close()?;
        let exit_status = channel.exit_status()?;

        Ok((s, exit_status))
    }

    pub fn has_active_session(&self) -> bool {
        self.session
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
            session: None,
        }
    }
}

impl Debug for RemoteConnection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RemoteConnection {{ connected: {} }}",
            self.session.as_ref().map_or(false, |s| s.authenticated())
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
