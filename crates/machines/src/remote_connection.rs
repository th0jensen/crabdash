use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use ssh2::Session;
use std::{
    fmt::{Debug, Formatter},
    io::Read,
    net::TcpStream,
};

#[derive(Default, Serialize, Deserialize)]
pub struct RemoteConnection {
    pub user: String,
    pub host: String,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub password: String,
    #[serde(skip)]
    session: Option<Session>,
}

impl RemoteConnection {
    pub fn new_connection(
        user: impl Into<String>,
        host: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<RemoteConnection> {
        let user = user.into();
        let host = host.into();
        let password = password.into();
        let mut rc = RemoteConnection {
            user,
            host,
            password,
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

        sess.userauth_password(&self.user, &self.password)?;
        if !sess.authenticated() {
            bail!("Authentication failed!");
        }
        Ok(sess)
    }

    pub fn ensure_connected(&mut self) -> Result<&Session> {
        if self.password.is_empty() {
            bail!("No password available");
        }
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
            password: self.password.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect() -> Result<()> {
        let rc = RemoteConnection {
            user: "thomas".to_string(),
            host: "prestige".to_string(),
            password: "tailscale".to_string(),
            session: None,
        };
        let sess = rc.connect()?;
        assert!(sess.authenticated());
        Ok(())
    }

    #[test]
    fn test_command() -> Result<()> {
        let mut rc = RemoteConnection::new_connection("thomas", "prestige", "tailscale")?;
        let (output, exit_status) = rc.run_ssh_command("ls", Some(&[&"-a"]))?;
        assert_eq!(exit_status, 0);
        assert!(!output.is_empty());
        Ok(())
    }
}
