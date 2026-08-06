use std::io;

use crate::{machine::Machine, remote_connection::RemoteConnection};
use anyhow::{Context as _, Result, anyhow};
use async_ssh2_lite::{AsyncChannel, TokioTcpStream, ssh2::ExtendedData};
use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use smol::{
    channel::{Receiver, Sender},
    io::{AsyncReadExt as _, AsyncWriteExt as _},
};

// Use a universally available terminal type: `xterm-ghostty` has no terminfo
// entry on stock macOS or most Linux servers, which breaks `clear` and any
// full-screen (ncurses) program in both local and SSH sessions.
pub const TERMINAL_TYPE: &str = "xterm-256color";

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
    pub async fn open_terminal(&mut self, size: TerminalSize) -> Result<TerminalSession> {
        match self.remote.as_mut() {
            Some(remote_connection) => remote_connection.open_terminal(size).await,
            None => open_local_terminal(size).await,
        }
    }
}

struct LocalTerminal {
    master: Box<dyn MasterPty + Send>,
    reader: smol::Unblock<Box<dyn io::Read + Send>>,
    writer: smol::Unblock<Box<dyn io::Write + Send>>,
    child: Box<dyn Child + Send + Sync>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

async fn open_local_terminal(size: TerminalSize) -> Result<TerminalSession> {
    let terminal = smol::unblock(move || -> Result<LocalTerminal> {
        let pty_pair = native_pty_system()
            .openpty(size.into())
            .context("failed to open local pseudo-terminal")?;
        let reader = pty_pair
            .master
            .try_clone_reader()
            .context("failed to clone pseudo-terminal reader")?;
        let writer = pty_pair
            .master
            .take_writer()
            .context("failed to take pseudo-terminal writer")?;

        let mut command = CommandBuilder::new_default_prog();
        command.env("TERM", TERMINAL_TYPE);
        command.env("COLORTERM", "truecolor");
        let child = pty_pair
            .slave
            .spawn_command(command)
            .context("failed to start local login shell")?;
        let killer = child.clone_killer();
        drop(pty_pair.slave);

        Ok(LocalTerminal {
            master: pty_pair.master,
            reader: smol::Unblock::with_capacity(64 * 1024, reader),
            writer: smol::Unblock::with_capacity(64 * 1024, writer),
            child,
            killer,
        })
    })
    .await?;

    let (commands, command_receiver) = smol::channel::unbounded();
    let (events, event_receiver) = smol::channel::unbounded();
    smol::spawn(run_local_terminal(terminal, command_receiver, events)).detach();

    Ok(TerminalSession {
        controller: TerminalController { commands },
        events: event_receiver,
    })
}

enum LocalAction {
    Output(io::Result<Vec<u8>>),
    Command(Result<TerminalCommand, smol::channel::RecvError>),
}

async fn run_local_terminal(
    mut terminal: LocalTerminal,
    commands: Receiver<TerminalCommand>,
    events: Sender<TerminalEvent>,
) {
    let mut natural_exit = false;

    loop {
        let action = smol::future::race(
            async {
                let mut buffer = vec![0; 16 * 1024];
                let result = terminal.reader.read(&mut buffer).await.map(|count| {
                    buffer.truncate(count);
                    buffer
                });
                LocalAction::Output(result)
            },
            async { LocalAction::Command(commands.recv().await) },
        )
        .await;

        match action {
            LocalAction::Output(Ok(output)) if output.is_empty() => {
                natural_exit = true;
                break;
            }
            LocalAction::Output(Ok(output)) => {
                if events.send(TerminalEvent::Output(output)).await.is_err() {
                    break;
                }
            }
            LocalAction::Output(Err(error)) => {
                if events
                    .send(TerminalEvent::Error(format!(
                        "Local terminal read failed: {error}"
                    )))
                    .await
                    .is_err()
                {
                    tracing::debug!("Terminal event receiver closed after local read failure");
                }
                break;
            }
            LocalAction::Command(Ok(TerminalCommand::Input(input))) => {
                if let Err(error) = terminal.writer.write_all(&input).await {
                    if events
                        .send(TerminalEvent::Error(format!(
                            "Local terminal write failed: {error}"
                        )))
                        .await
                        .is_err()
                    {
                        tracing::debug!("Terminal event receiver closed after local write failure");
                    }
                    break;
                }
                if let Err(error) = terminal.writer.flush().await {
                    if events
                        .send(TerminalEvent::Error(format!(
                            "Local terminal flush failed: {error}"
                        )))
                        .await
                        .is_err()
                    {
                        tracing::debug!("Terminal event receiver closed after local flush failure");
                    }
                    break;
                }
            }
            LocalAction::Command(Ok(TerminalCommand::Resize(size))) => {
                if let Err(error) = terminal.master.resize(size.into()) {
                    if events
                        .send(TerminalEvent::Error(format!(
                            "Local terminal resize failed: {error}"
                        )))
                        .await
                        .is_err()
                    {
                        tracing::debug!(
                            "Terminal event receiver closed after local resize failure"
                        );
                    }
                }
            }
            LocalAction::Command(Ok(TerminalCommand::Shutdown) | Err(_)) => break,
        }
    }

    if let Err(error) = terminal.writer.flush().await {
        tracing::debug!(%error, "Failed to flush local terminal during shutdown");
    }
    drop(terminal.writer);

    if !natural_exit {
        if let Err(error) = terminal.killer.kill() {
            tracing::debug!(%error, "Failed to terminate local terminal process");
        }
    }

    let exit_status = smol::unblock(move || terminal.child.wait()).await;
    match exit_status {
        Ok(status) => {
            let code = status.exit_code() as i32;
            if events
                .send(TerminalEvent::Exited(Some(code)))
                .await
                .is_err()
            {
                tracing::debug!("Terminal event receiver closed before local exit status");
            }
        }
        Err(error) => {
            if events
                .send(TerminalEvent::Error(format!(
                    "Failed to reap local terminal process: {error}"
                )))
                .await
                .is_err()
            {
                tracing::debug!("Terminal event receiver closed before local wait error");
            }
        }
    }
}

impl RemoteConnection {
    async fn open_terminal(&mut self, size: TerminalSize) -> Result<TerminalSession> {
        let session = self.connect().await?;
        let channel = super::remote_connection::ssh_runtime()
            .spawn(async move {
                let mut channel = session.channel_session().await?;
                channel.handle_extended_data(ExtendedData::Merge).await?;
                channel
                    .request_pty(
                        TERMINAL_TYPE,
                        None,
                        Some((
                            u32::from(size.columns),
                            u32::from(size.rows),
                            u32::from(size.pixel_width),
                            u32::from(size.pixel_height),
                        )),
                    )
                    .await?;
                channel.shell().await?;
                Ok::<_, anyhow::Error>(channel)
            })
            .await
            .map_err(|error| anyhow!("SSH terminal task panicked: {error}"))??;

        let (commands, command_receiver) = smol::channel::unbounded();
        let (events, event_receiver) = smol::channel::unbounded();
        super::remote_connection::ssh_runtime().spawn(run_remote_terminal(
            channel,
            command_receiver,
            events,
        ));

        Ok(TerminalSession {
            controller: TerminalController { commands },
            events: event_receiver,
        })
    }
}

async fn run_remote_terminal(
    mut channel: AsyncChannel<TokioTcpStream>,
    commands: Receiver<TerminalCommand>,
    events: Sender<TerminalEvent>,
) {
    let mut buffer = vec![0; 16 * 1024];

    loop {
        tokio::select! {
            output = channel.read(&mut buffer) => {
                match output {
                    Ok(0) => break,
                    Ok(count) => {
                        if events
                            .send(TerminalEvent::Output(buffer[..count].to_vec()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(TerminalEvent::Error(format!("SSH terminal read failed: {error}")))
                            .await
                            .is_err()
                        {
                            tracing::debug!("Terminal event receiver closed after SSH read failure");
                        }
                        break;
                    }
                }
            }
            command = commands.recv() => {
                match command {
                    Ok(TerminalCommand::Input(input)) => {
                        if let Err(error) = tokio::io::AsyncWriteExt::write_all(&mut channel, &input).await {
                            if events
                                .send(TerminalEvent::Error(format!("SSH terminal write failed: {error}")))
                                .await
                                .is_err()
                            {
                                tracing::debug!("Terminal event receiver closed after SSH write failure");
                            }
                            break;
                        }
                        if let Err(error) = tokio::io::AsyncWriteExt::flush(&mut channel).await {
                            if events
                                .send(TerminalEvent::Error(format!("SSH terminal flush failed: {error}")))
                                .await
                                .is_err()
                            {
                                tracing::debug!("Terminal event receiver closed after SSH flush failure");
                            }
                            break;
                        }
                    }
                    Ok(TerminalCommand::Resize(size)) => {
                        if let Err(error) = channel
                            .request_pty_size(
                                u32::from(size.columns),
                                u32::from(size.rows),
                                (size.pixel_width > 0).then(|| u32::from(size.pixel_width)),
                                (size.pixel_height > 0).then(|| u32::from(size.pixel_height)),
                            )
                            .await
                        {
                            if events
                                .send(TerminalEvent::Error(format!("SSH terminal resize failed: {error}")))
                                .await
                                .is_err()
                            {
                                tracing::debug!("Terminal event receiver closed after SSH resize failure");
                            }
                        }
                    }
                    Ok(TerminalCommand::Shutdown) | Err(_) => break,
                }
            }
        }
    }

    if let Err(error) = channel.send_eof().await {
        tracing::debug!(%error, "Failed to send SSH terminal EOF");
    }
    if let Err(error) = channel.close().await {
        tracing::debug!(%error, "Failed to close SSH terminal channel");
    }
    if let Err(error) = channel.wait_close().await {
        tracing::debug!(%error, "Failed waiting for SSH terminal channel to close");
    }

    match channel.exit_status() {
        Ok(status) => {
            if events
                .send(TerminalEvent::Exited(Some(status)))
                .await
                .is_err()
            {
                tracing::debug!("Terminal event receiver closed before SSH exit status");
            }
        }
        Err(error) => {
            if events
                .send(TerminalEvent::Error(format!(
                    "Failed to read SSH terminal exit status: {error}"
                )))
                .await
                .is_err()
            {
                tracing::debug!("Terminal event receiver closed before SSH exit error");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{TerminalEvent, TerminalSize, open_local_terminal};

    #[test]
    fn local_terminal_streams_stdin_and_stdout() -> anyhow::Result<()> {
        smol::block_on(async {
            let session = open_local_terminal(TerminalSize::default()).await?;
            session
                .controller
                .write(b"printf 'crabdash-terminal-ok\\n'; exit\r".to_vec())?;

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
            Ok(())
        })
    }
}
