use anyhow::{Result, bail};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use ssh2::Session;
use std::{io::Read, net::TcpStream};

const KEYRING_SERVICE: &str = "com.thojensen.crabdash.remote";

#[derive(Default, Serialize, Deserialize)]
pub struct RemoteConnection {
    user: String,
    host: String,
    #[serde(skip_serializing, skip_deserializing, default)]
    password: String,
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
        let sess = Self::connect(&user, &host, &password)?;
        Ok(RemoteConnection {
            user,
            host,
            password,
            session: Some(sess),
        })
    }

    pub fn store_password(&self) -> Result<()> {
        let entry = self.keyring_entry()?;
        entry.set_password(&self.password)?;
        Ok(())
    }

    pub fn connect(user: &str, host: &str, password: &str) -> Result<Session> {
        let tcp = TcpStream::connect(format!("{host}:22"))?;
        let mut sess = Session::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;
        sess.userauth_password(user, password)?;

        if !sess.authenticated() {
            bail!("Authentication failed!");
        }

        Ok(sess)
    }

    pub fn ensure_connected(&mut self) -> Result<&Session> {
        if self.password.is_empty() {
            self.password = self.keyring_entry()?.get_password()?;
        }

        if self.session.is_none() {
            self.session = Some(Self::connect(&self.user, &self.host, &self.password)?);
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

        eprintln!("Command: {}", full_cmd);
        channel.exec(&full_cmd)?;

        let mut s = String::new();
        channel.read_to_string(&mut s)?;
        eprintln!("Output: {}", s);

        channel.wait_close()?;
        let exit_status = channel.exit_status()?;

        Ok((s, exit_status))
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

    fn keyring_entry(&self) -> Result<Entry, keyring::Error> {
        Entry::new(KEYRING_SERVICE, &format!("{}@{}", self.user, self.host))
    }
}

impl std::fmt::Debug for RemoteConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
        let sess = RemoteConnection::connect("thomas", "prestige", "")?;
        assert!(sess.authenticated());
        Ok(())
    }

    #[test]
    fn test_command() -> Result<()> {
        let mut rc = RemoteConnection::new_connection("thomas", "prestige", "")?;
        let (output, exit_status) = rc.run_ssh_command("ls", Some(&[&"-a"]))?;
        assert_eq!(exit_status, 0);
        assert!(!output.is_empty());
        Ok(())
    }
}
