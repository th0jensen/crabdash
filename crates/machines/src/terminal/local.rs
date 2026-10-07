use super::{
    TerminalCommand, TerminalController, TerminalEvent, TerminalOptions, TerminalSession,
    TerminalSize,
};
use anyhow::{Context as _, Result};
use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, native_pty_system};
use smol::{
    channel::{Receiver, Sender},
    io::{AsyncReadExt as _, AsyncWriteExt as _},
};
use std::io;
struct LocalTerminal {
    master: Box<dyn MasterPty + Send>,
    reader: smol::Unblock<Box<dyn io::Read + Send>>,
    writer: smol::Unblock<Box<dyn io::Write + Send>>,
    child: Box<dyn Child + Send + Sync>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

pub(super) async fn open_local_terminal(
    size: TerminalSize,
    options: TerminalOptions,
) -> Result<TerminalSession> {
    options.validate()?;
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
        command.env("TERM", &options.terminal_type);
        if options.true_color {
            command.env("COLORTERM", "truecolor");
        } else {
            command.env_remove("COLORTERM");
        }
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
