//! Terminal session API; local PTY and SSH transports share this contract.
mod local;
mod ssh;
use crate::machine::Machine;
use anyhow::{Result, anyhow};
use portable_pty::PtySize;
use smol::channel::{Receiver, Sender};

// Use a universally available terminal type: `xterm-ghostty` has no terminfo
// entry on stock macOS or most Linux servers, which breaks `clear` and any
// full-screen (ncurses) program in both local and SSH sessions.
pub const TERMINAL_TYPE: &str = "xterm-256color";

#[derive(Clone, Debug)]
pub struct TerminalOptions {
    pub terminal_type: String,
    pub true_color: bool,
}
impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            terminal_type: TERMINAL_TYPE.into(),
            true_color: true,
        }
    }
}
impl TerminalOptions {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            !self.terminal_type.is_empty()
                && self.terminal_type.len() <= 64
                && self
                    .terminal_type
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.+".contains(&byte)),
            "Terminal type must be a terminfo name using letters, numbers, -, _, . or +."
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            columns: 120,
            rows: 24,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

impl From<TerminalSize> for PtySize {
    fn from(size: TerminalSize) -> Self {
        Self {
            rows: size.rows,
            cols: size.columns,
            pixel_width: size.pixel_width,
            pixel_height: size.pixel_height,
        }
    }
}

#[derive(Debug)]
pub enum TerminalEvent {
    Output(Vec<u8>),
    Exited(Option<i32>),
    Error(String),
}

#[derive(Debug)]
enum TerminalCommand {
    Input(Vec<u8>),
    Resize(TerminalSize),
    Shutdown,
}

#[derive(Clone, Debug)]
pub struct TerminalController {
    commands: Sender<TerminalCommand>,
}

impl TerminalController {
    pub fn write(&self, bytes: impl Into<Vec<u8>>) -> Result<()> {
        self.commands
            .try_send(TerminalCommand::Input(bytes.into()))
            .map_err(|error| anyhow!("terminal input channel closed: {error}"))
    }

    pub fn resize(&self, size: TerminalSize) -> Result<()> {
        self.commands
            .try_send(TerminalCommand::Resize(size))
            .map_err(|error| anyhow!("terminal resize channel closed: {error}"))
    }

    pub fn shutdown(&self) -> Result<()> {
        self.commands
            .try_send(TerminalCommand::Shutdown)
            .map_err(|error| anyhow!("terminal command channel closed: {error}"))
    }
}

pub struct TerminalSession {
    pub controller: TerminalController,
    pub events: Receiver<TerminalEvent>,
}

impl Machine {
    pub async fn open_terminal(
        &mut self,
        size: TerminalSize,
        options: TerminalOptions,
    ) -> Result<TerminalSession> {
        options.validate()?;
        match self.remote.as_mut() {
            Some(remote_connection) => remote_connection.open_terminal(size, options).await,
            None => local::open_local_terminal(size, options).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{TerminalEvent, TerminalSize, local::open_local_terminal};

    #[test]
    fn local_terminal_streams_stdin_and_stdout() -> anyhow::Result<()> {
        terminal_round_trip(
            super::TerminalOptions::default(),
            "crabdash-env=xterm-256color:truecolor",
        )
    }

    #[test]
    fn custom_terminfo_and_disabled_true_color_reach_the_shell() -> anyhow::Result<()> {
        terminal_round_trip(
            super::TerminalOptions {
                terminal_type: "vt100".into(),
                true_color: false,
            },
            "crabdash-env=vt100:",
        )
    }

    fn terminal_round_trip(options: super::TerminalOptions, expected: &str) -> anyhow::Result<()> {
        smol::block_on(async {
            let session = open_local_terminal(TerminalSize::default(), options).await?;
            session
                .controller
                .write(b"printf 'crabdash-terminal-ok\\ncrabdash-env=%s:%s\\n' \"$TERM\" \"${COLORTERM-}\"; exit\r".to_vec())?;

            let mut output = Vec::new();
            loop {
                let event = smol::future::race(
                    async { session.events.recv().await.map_err(anyhow::Error::from) },
                    async {
                        smol::Timer::after(Duration::from_secs(5)).await;
                        Err(anyhow::anyhow!("timed out waiting for terminal output"))
                    },
                )
                .await?;

                match event {
                    TerminalEvent::Output(bytes) => output.extend(bytes),
                    TerminalEvent::Exited(_) => break,
                    TerminalEvent::Error(error) => return Err(anyhow::anyhow!(error)),
                }
            }

            let output = String::from_utf8_lossy(&output);
            anyhow::ensure!(
                output.contains("crabdash-terminal-ok"),
                "terminal output did not contain command result: {output}"
            );
            anyhow::ensure!(
                output.contains(expected),
                "terminal environment was wrong: {output}"
            );
            Ok(())
        })
    }
}
