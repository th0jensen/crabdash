use super::LocalTerminal;
use crate::terminal::{TerminalCommand, TerminalEvent};
use anyhow::Result;
use smol::{
    channel::{Receiver, Sender},
    io::{AsyncReadExt as _, AsyncWriteExt as _},
};
use std::io;

enum LocalAction {
    Output(io::Result<Vec<u8>>),
    Command(Result<TerminalCommand, smol::channel::RecvError>),
}

pub(super) async fn run_local_terminal(
    mut terminal: LocalTerminal,
    commands: Receiver<TerminalCommand>,
    events: Sender<TerminalEvent>,
) {
    let mut reader = smol::Unblock::with_capacity(64 * 1024, terminal.reader);
    let mut writer = smol::Unblock::with_capacity(64 * 1024, terminal.writer);
    let mut natural_exit = false;

    loop {
        let action = smol::future::race(
            async {
                let mut buffer = vec![0; 16 * 1024];
                let result = reader.read(&mut buffer).await.map(|count| {
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
                if let Err(error) = writer.write_all(&input).await {
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
                if let Err(error) = writer.flush().await {
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

    if let Err(error) = writer.flush().await {
        tracing::debug!(%error, "Failed to flush local terminal during shutdown");
    }
    drop(writer);

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
