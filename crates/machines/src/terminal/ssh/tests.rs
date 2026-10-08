//! Real transport regression; run only with the isolated loopback sshd fixture.
use super::run_remote_terminal;
use crate::{
    command::AbortOnDrop,
    terminal::{TerminalCommand, TerminalEvent, TerminalSize},
};
use anyhow::{Context as _, Result, anyhow, ensure};
use async_ssh2_lite::{AsyncSession, TokioTcpStream, ssh2::ExtendedData};
use std::{path::PathBuf, time::Duration};
use tokio::io::AsyncReadExt as _;

#[tokio::test]
#[ignore = "requires isolated loopback sshd and explicit CRABDASH_SSH_DEADLINE_TEST_KEY"]
async fn input_preserves_buffered_remote_terminal_output() -> Result<()> {
    let private = PathBuf::from(std::env::var("CRABDASH_SSH_DEADLINE_TEST_KEY")?);
    let public = std::env::var_os("CRABDASH_SSH_DEADLINE_TEST_PUBKEY").map(PathBuf::from);
    tokio::time::timeout(Duration::from_secs(10), async {
        // Every network operation is owned by this bounded test future. No
        // normal account credentials or detached connection task are used.
        let tcp = TokioTcpStream::connect("127.0.0.1:22").await?;
        let mut session = AsyncSession::new(tcp, None)?;
        session.handshake().await?;
        session.userauth_pubkey_file("thomas", public.as_deref(), &private, None).await?;
        ensure!(session.authenticated(), "fixture authentication failed");
        let mut channel = session.channel_session().await?;
        channel.handle_extended_data(ExtendedData::Merge).await?;
        let size = TerminalSize::default();
        channel.request_pty("xterm-256color", None, Some((
            u32::from(size.columns), u32::from(size.rows), 0, 0,
        ))).await?;
        const PAYLOAD: usize = 1024 * 1024;
        const PENDING: u32 = 512 * 1024;
        // SSH executes through the account's login shell, which need not be
        // POSIX. Keep the shell command to one quoted argument, and configure
        // the PTY inside the peer rather than relying on shell operators/stty.
        channel.exec("python3 -u -c 'import sys, tty; tty.setraw(sys.stdin.fileno()); sys.stdout.buffer.write(b\"z\"*1048576); sys.stdout.flush(); line=sys.stdin.buffer.readline(); sys.stdout.buffer.write(b\"ACK:\"+line); sys.stdout.flush()'").await?;

        // A tiny read drives real libssh2 packet reception. Preserve those
        // consumed bytes and wait until there is substantial unread native
        // output before allowing the actor to process the queued input.
        let mut received = Vec::with_capacity(PAYLOAD + 16);
        let mut byte = [0];
        while channel.read_window().available < PENDING {
            if channel.read(&mut byte).await? == 0 {
                let preview = String::from_utf8_lossy(&received[..received.len().min(512)]);
                return Err(anyhow!(
                    "peer ended before the queued-output precondition: received {} bytes; exit status {:?}; output preview {preview:?}",
                    received.len(), channel.exit_status(),
                ));
            }
            received.extend(byte);
            ensure!(received.len() < PAYLOAD, "peer payload was exhausted before native output buffered");
            tokio::task::yield_now().await;
        }
        let (commands, command_receiver) = smol::channel::unbounded();
        let (events, event_receiver) = smol::channel::unbounded();
        commands.try_send(TerminalCommand::Input(b"ping\n".to_vec()))?;
        let actor = AbortOnDrop::new(tokio::spawn(run_remote_terminal(channel, command_receiver, events)));
        loop {
            match event_receiver.recv().await.context("terminal actor closed without exit")? {
                TerminalEvent::Output(bytes) => received.extend(bytes),
                TerminalEvent::Exited(code) => { ensure!(code == Some(0), "unexpected terminal exit: {code:?}"); break; }
                TerminalEvent::Error(message) => return Err(anyhow!(message)),
            }
        }
        actor.await.context("terminal actor failed")?;
        let mut expected = vec![b'z'; PAYLOAD];
        expected.extend(b"ACK:ping\n");
        ensure!(received == expected, "terminal output lost around input: received {} of {} bytes", received.len(), expected.len());
        // The actor's select order is intentionally unchanged. At least 32
        // read chunks are queued before input; verify the original flush is a
        // failing negative control when this isolated test is run.
        Ok::<_, anyhow::Error>(())
    }).await.context("remote terminal output regression timed out")?
}
