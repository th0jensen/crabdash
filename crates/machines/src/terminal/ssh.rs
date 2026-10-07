use super::{
    TerminalCommand, TerminalController, TerminalEvent, TerminalOptions, TerminalSession,
    TerminalSize,
};
use crate::remote_connection::RemoteConnection;
use anyhow::{Result, anyhow};
use async_ssh2_lite::{AsyncChannel, TokioTcpStream, ssh2::ExtendedData};
use smol::{
    channel::{Receiver, Sender},
    io::AsyncReadExt as _,
};
impl RemoteConnection {
    pub(super) async fn open_terminal(
        &mut self,
        size: TerminalSize,
        options: TerminalOptions,
    ) -> Result<TerminalSession> {
        let session = self.connect().await?;
        let runtime = crate::remote_connection::ssh_runtime()?;
        let channel = runtime
            .spawn(async move {
                let mut channel = session.channel_session().await?;
                channel.handle_extended_data(ExtendedData::Merge).await?;
                channel
                    .request_pty(
                        &options.terminal_type,
                        None,
                        Some((
                            u32::from(size.columns),
                            u32::from(size.rows),
                            u32::from(size.pixel_width),
                            u32::from(size.pixel_height),
                        )),
                    )
                    .await?;
                // Servers often reject environment requests; TERM is already supplied by request_pty.
                if let Err(error) = channel
                    .setenv(
                        "COLORTERM",
                        if options.true_color { "truecolor" } else { "" },
                    )
                    .await
                {
                    tracing::debug!(%error, "SSH server declined COLORTERM");
                }
                channel.shell().await?;
                Ok::<_, anyhow::Error>(channel)
            })
            .await
            .map_err(|error| anyhow!("SSH terminal task panicked: {error}"))??;

        let (commands, command_receiver) = smol::channel::unbounded();
        let (events, event_receiver) = smol::channel::unbounded();
        runtime.spawn(run_remote_terminal(channel, command_receiver, events));

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
