//! ConPTY's synchronous input, output and control operations are serviced
//! independently. Native master close can wait for output to drain and belongs
//! on the control worker, never the UI/executor thread. The vendored backend
//! uses standard creation flags, so all VT cursor queries remain UI-owned.
use super::LocalTerminal;
use crate::terminal::{TerminalCommand, TerminalEvent, TerminalSize};
use smol::channel::{Receiver, Sender};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

enum Completion {
    ReaderFinished,
    WriterFailed,
    ChildFinished,
}
enum Action {
    Command(Result<TerminalCommand, smol::channel::RecvError>),
    Worker(Result<Completion, smol::channel::RecvError>),
    ReceiverClosed,
}

pub(super) async fn run_local_terminal(
    terminal: LocalTerminal,
    commands: Receiver<TerminalCommand>,
    events: Sender<TerminalEvent>,
) {
    let LocalTerminal {
        master,
        mut reader,
        mut writer,
        mut child,
        mut killer,
    } = terminal;
    let stopping = Arc::new(AtomicBool::new(false));
    let (inputs, input_receiver) = smol::channel::unbounded::<Vec<u8>>();
    let (resizes, resize_receiver) = smol::channel::unbounded::<TerminalSize>();
    let (completion, completed) = smol::channel::unbounded();

    let reader_events = events.clone();
    let reader_completion = completion.clone();
    let reader_task = smol::spawn(smol::unblock(move || {
        let mut buffer = [0; 16 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    // Keep draining even when the UI has gone: native close
                    // may be waiting for the pipe to become writable.
                    let _ = reader_events.try_send(TerminalEvent::Output(buffer[..count].to_vec()));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    report(
                        &reader_events,
                        format!("Local terminal read failed: {error}"),
                    );
                    break;
                }
            }
        }
        drop(reader);
        let _ = reader_completion.try_send(Completion::ReaderFinished);
    }));

    let writer_stopping = stopping.clone();
    let writer_events = events.clone();
    let writer_completion = completion.clone();
    let writer_task = smol::spawn(smol::unblock(move || {
        let result = (|| -> io::Result<()> {
            while let Ok(input) = input_receiver.recv_blocking() {
                if writer_stopping.load(Ordering::Acquire) {
                    break;
                }
                writer.write_all(&input)?;
                writer.flush()?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            if !writer_stopping.load(Ordering::Acquire) {
                report(
                    &writer_events,
                    format!("Local terminal write failed: {error}"),
                );
                let _ = writer_completion.try_send(Completion::WriterFailed);
            }
        }
        // Dropping a portable writer may itself perform native I/O.
        drop(writer);
    }));

    let control_stopping = stopping.clone();
    let control_events = events.clone();
    let control_task = smol::spawn(smol::unblock(move || {
        while let Ok(size) = resize_receiver.recv_blocking() {
            if control_stopping.load(Ordering::Acquire) {
                break;
            }
            if let Err(error) = master.resize(size.into()) {
                if !control_stopping.load(Ordering::Acquire) {
                    report(
                        &control_events,
                        format!("Local terminal resize failed: {error}"),
                    );
                }
            }
        }
        drop(master);
    }));
    let child_task = smol::spawn(smol::unblock(move || {
        let status = child.wait();
        let _ = completion.try_send(Completion::ChildFinished);
        status
    }));

    let natural_exit = loop {
        let action = smol::future::race(
            async { Action::Command(commands.recv().await) },
            smol::future::race(async { Action::Worker(completed.recv().await) }, async {
                events.closed().await;
                Action::ReceiverClosed
            }),
        )
        .await;
        match action {
            Action::Command(Ok(TerminalCommand::Input(input))) => {
                let _ = inputs.try_send(input);
            }
            Action::Command(Ok(TerminalCommand::Resize(size))) => {
                let _ = resizes.try_send(size);
            }
            Action::Worker(Ok(Completion::ChildFinished)) => break true,
            Action::Command(Ok(TerminalCommand::Shutdown) | Err(_))
            | Action::Worker(Ok(Completion::ReaderFinished | Completion::WriterFailed) | Err(_))
            | Action::ReceiverClosed => break false,
        }
    };

    stopping.store(true, Ordering::Release);
    commands.close();
    inputs.close();
    resizes.close();
    // Killing/reaping runs independently of any blocked resize or write.
    smol::unblock(move || {
        if !natural_exit {
            if let Err(error) = killer.kill() {
                tracing::debug!(%error, "Failed to terminate local terminal process");
            }
        }
    })
    .await;
    // These owned workers are already running concurrently. Close the master
    // while the reader drains, then publish exit only after all output and I/O.
    control_task.await;
    reader_task.await;
    writer_task.await;
    match child_task.await {
        Ok(status) => {
            let _ = events.try_send(TerminalEvent::Exited(Some(status.exit_code() as i32)));
        }
        Err(error) => report(
            &events,
            format!("Failed to reap local terminal process: {error}"),
        ),
    }
}

fn report(events: &Sender<TerminalEvent>, message: String) {
    let _ = events.try_send(TerminalEvent::Error(message));
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, anyhow, ensure};
    use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
    use std::{
        sync::{Condvar, Mutex, MutexGuard, mpsc},
        time::Duration,
    };

    #[derive(Default, Debug)]
    struct State {
        killed: bool,
        master_closed: bool,
        cursor_replied: bool,
        tail_read: bool,
        writes: Vec<Vec<u8>>,
        reader_dropped: bool,
        writer_dropped: bool,
        child_dropped: bool,
        close_drained: bool,
        exited: bool,
    }
    #[derive(Clone, Debug)]
    struct Shared {
        state: Arc<Mutex<State>>,
        changed: Arc<Condvar>,
        output: mpsc::Sender<Vec<u8>>,
        probes: Sender<Probe>,
    }
    #[derive(Debug)]
    enum Probe {
        ResizeStarted,
        WriteBlocked,
        Written(Vec<u8>),
        Killed,
    }
    fn lock(shared: &Shared) -> MutexGuard<'_, State> {
        match shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
    fn wait(shared: &Shared, predicate: impl Fn(&State) -> bool) -> io::Result<()> {
        let result =
            shared
                .changed
                .wait_timeout_while(lock(shared), Duration::from_secs(3), |state| {
                    !predicate(state)
                });
        let (state, _) = match result {
            Ok(result) => result,
            Err(poisoned) => poisoned.into_inner(),
        };
        if predicate(&state) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "fake PTY worker did not progress",
            ))
        }
    }
    struct Reader {
        shared: Shared,
        output: mpsc::Receiver<Vec<u8>>,
    }
    impl io::Read for Reader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let bytes = self
                .output
                .recv_timeout(Duration::from_secs(3))
                .map_err(io::Error::other)?;
            if bytes == b"tail" {
                lock(&self.shared).tail_read = true;
                self.shared.changed.notify_all();
            }
            if bytes.len() > buffer.len() {
                return Err(io::Error::other("fake read buffer too small"));
            }
            buffer[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
    }
    impl Drop for Reader {
        fn drop(&mut self) {
            lock(&self.shared).reader_dropped = true;
        }
    }
    struct Writer(Shared);
    impl io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.starts_with(b"paste") {
                let _ = self.0.probes.try_send(Probe::WriteBlocked);
                wait(&self.0, |state| state.master_closed)?;
            }
            let mut state = lock(&self.0);
            state.writes.push(bytes.to_vec());
            if bytes == b"\x1b[1;1R" {
                state.cursor_replied = true;
            }
            drop(state);
            self.0.changed.notify_all();
            let _ = self.0.probes.try_send(Probe::Written(bytes.to_vec()));
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Drop for Writer {
        fn drop(&mut self) {
            lock(&self.0).writer_dropped = true;
        }
    }
    struct Master(Shared);
    impl MasterPty for Master {
        fn resize(&self, _: PtySize) -> Result<()> {
            let _ = self.0.probes.try_send(Probe::ResizeStarted);
            self.0.output.send(b"\x1b[".to_vec())?;
            self.0.output.send(b"6n".to_vec())?;
            wait(&self.0, |state| state.cursor_replied || state.killed)?;
            Ok(())
        }
        fn get_size(&self) -> Result<PtySize> {
            Ok(PtySize::default())
        }
        fn try_clone_reader(&self) -> Result<Box<dyn io::Read + Send>> {
            Err(anyhow!("reader already assigned"))
        }
        fn take_writer(&self) -> Result<Box<dyn io::Write + Send>> {
            Err(anyhow!("writer already assigned"))
        }
        #[cfg(unix)]
        fn process_group_leader(&self) -> Option<i32> {
            None
        }
        #[cfg(unix)]
        fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
            None
        }
        #[cfg(unix)]
        fn tty_name(&self) -> Option<std::path::PathBuf> {
            None
        }
    }
    impl Drop for Master {
        fn drop(&mut self) {
            lock(&self.0).master_closed = true;
            self.0.changed.notify_all();
            let _ = self.0.output.send(b"tail".to_vec());
            let drained = wait(&self.0, |state| state.tail_read).is_ok();
            lock(&self.0).close_drained = drained;
            let _ = self.0.output.send(Vec::new());
        }
    }
    #[derive(Debug)]
    struct Process(Shared);
    impl ChildKiller for Process {
        fn kill(&mut self) -> io::Result<()> {
            lock(&self.0).killed = true;
            self.0.changed.notify_all();
            let _ = self.0.probes.try_send(Probe::Killed);
            Ok(())
        }
        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(Killer(self.0.clone()))
        }
    }
    impl Child for Process {
        fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            let state = lock(&self.0);
            Ok((state.killed || state.exited).then(|| ExitStatus::with_exit_code(7)))
        }
        fn wait(&mut self) -> io::Result<ExitStatus> {
            wait(&self.0, |state| state.killed || state.exited)?;
            Ok(ExitStatus::with_exit_code(7))
        }
        fn process_id(&self) -> Option<u32> {
            None
        }
        #[cfg(windows)]
        fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
            None
        }
    }
    impl Drop for Process {
        fn drop(&mut self) {
            lock(&self.0).child_dropped = true;
        }
    }
    #[derive(Debug)]
    struct Killer(Shared);
    impl ChildKiller for Killer {
        fn kill(&mut self) -> io::Result<()> {
            lock(&self.0).killed = true;
            self.0.changed.notify_all();
            let _ = self.0.probes.try_send(Probe::Killed);
            Ok(())
        }
        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(Self(self.0.clone()))
        }
    }
    fn fixture() -> (LocalTerminal, Shared, Receiver<Probe>) {
        let (output, read) = mpsc::channel();
        let (probes, observed) = smol::channel::unbounded();
        let shared = Shared {
            state: Arc::default(),
            changed: Arc::default(),
            output,
            probes,
        };
        let terminal = LocalTerminal {
            master: Box::new(Master(shared.clone())),
            reader: Box::new(Reader {
                shared: shared.clone(),
                output: read,
            }),
            writer: Box::new(Writer(shared.clone())),
            child: Box::new(Process(shared.clone())),
            killer: Box::new(Killer(shared.clone())),
        };
        (terminal, shared, observed)
    }
    async fn next<T>(receiver: &Receiver<T>) -> Result<T> {
        smol::future::race(
            async { receiver.recv().await.map_err(anyhow::Error::from) },
            async {
                smol::Timer::after(Duration::from_secs(2)).await;
                Err(anyhow!("coordinator test timed out"))
            },
        )
        .await
    }
    async fn finish(
        task: smol::Task<()>,
        events: &Receiver<TerminalEvent>,
        shared: &Shared,
        expect_killed: bool,
    ) -> Result<()> {
        let mut output = Vec::new();
        loop {
            match next(events).await? {
                TerminalEvent::Output(bytes) => output.extend(bytes),
                TerminalEvent::Exited(code) => {
                    ensure!(code == Some(7));
                    break;
                }
                TerminalEvent::Error(error) => return Err(anyhow!(error)),
            }
        }
        ensure!(
            output.ends_with(b"tail"),
            "final output was lost before exit"
        );
        task.await;
        let state = lock(shared);
        ensure!(state.killed == expect_killed && state.close_drained);
        ensure!(state.reader_dropped && state.writer_dropped && state.child_dropped);
        Ok(())
    }

    #[test]
    fn resize_allows_cursor_reply_and_fifo_input_while_output_is_read() -> Result<()> {
        smol::block_on(async {
            let (terminal, shared, probes) = fixture();
            let (commands, receiver) = smol::channel::unbounded();
            let (events, output) = smol::channel::unbounded();
            let task = smol::spawn(run_local_terminal(terminal, receiver, events));
            commands.try_send(TerminalCommand::Resize(TerminalSize::default()))?;
            ensure!(matches!(next(&probes).await?, Probe::ResizeStarted));
            let mut query = Vec::new();
            while query.len() < 4 {
                match next(&output).await? {
                    TerminalEvent::Output(bytes) => query.extend(bytes),
                    other => return Err(anyhow!("unexpected cursor-query event {other:?}")),
                }
            }
            ensure!(query == b"\x1b[6n");
            for input in [b"\x1b[1;1R".to_vec(), b"first".to_vec(), b"second".to_vec()] {
                commands.try_send(TerminalCommand::Input(input.clone()))?;
                match next(&probes).await? {
                    Probe::Written(bytes) => ensure!(bytes == input),
                    other => return Err(anyhow!("unexpected writer probe {other:?}")),
                }
            }
            commands.try_send(TerminalCommand::Shutdown)?;
            finish(task, &output, &shared, true).await
        })
    }

    #[test]
    fn shutdown_does_not_wait_for_a_blocked_large_write() -> Result<()> {
        smol::block_on(async {
            let (terminal, shared, probes) = fixture();
            let (commands, receiver) = smol::channel::unbounded();
            let (events, output) = smol::channel::unbounded();
            let task = smol::spawn(run_local_terminal(terminal, receiver, events));
            commands.try_send(TerminalCommand::Input(
                [b"paste".as_slice(), &vec![b'x'; 128 * 1024]].concat(),
            ))?;
            ensure!(matches!(next(&probes).await?, Probe::WriteBlocked));
            commands.try_send(TerminalCommand::Input(b"discard".to_vec()))?;
            commands.try_send(TerminalCommand::Shutdown)?;
            finish(task, &output, &shared, true).await?;
            ensure!(
                lock(&shared).writes.len() == 1,
                "queued ordinary input was written during shutdown"
            );
            Ok(())
        })
    }

    #[test]
    fn shutdown_while_resize_waits_does_not_require_a_ui_cursor_reply() -> Result<()> {
        smol::block_on(async {
            let (terminal, shared, probes) = fixture();
            let (commands, receiver) = smol::channel::unbounded();
            let (events, output) = smol::channel::unbounded();
            let task = smol::spawn(run_local_terminal(terminal, receiver, events));
            commands.try_send(TerminalCommand::Resize(TerminalSize::default()))?;
            ensure!(matches!(next(&probes).await?, Probe::ResizeStarted));
            commands.try_send(TerminalCommand::Shutdown)?;
            finish(task, &output, &shared, true).await?;
            ensure!(lock(&shared).writes.is_empty());
            Ok(())
        })
    }

    #[test]
    fn rejected_session_closes_without_a_cursor_handshake() -> Result<()> {
        smol::block_on(async {
            let (terminal, shared, _) = fixture();
            let (commands, receiver) = smol::channel::unbounded();
            let (events, output) = smol::channel::unbounded();
            commands.try_send(TerminalCommand::Shutdown)?;
            let task = smol::spawn(run_local_terminal(terminal, receiver, events));
            finish(task, &output, &shared, true).await?;
            ensure!(lock(&shared).writes.is_empty());
            Ok(())
        })
    }

    #[test]
    fn dropping_event_receiver_still_drains_and_closes_the_session() -> Result<()> {
        smol::block_on(async {
            let (terminal, shared, _) = fixture();
            let (_commands, receiver) = smol::channel::unbounded();
            let (events, output) = smol::channel::unbounded();
            drop(output);
            let task = smol::spawn(run_local_terminal(terminal, receiver, events));
            smol::future::race(
                async {
                    task.await;
                    Ok::<_, anyhow::Error>(())
                },
                async {
                    smol::Timer::after(Duration::from_secs(2)).await;
                    Err(anyhow!("dropped session was not cleaned up"))
                },
            )
            .await?;
            let state = lock(&shared);
            ensure!(state.killed && state.close_drained && state.master_closed);
            ensure!(state.reader_dropped && state.writer_dropped && state.child_dropped);
            ensure!(state.writes.is_empty());
            Ok(())
        })
    }

    #[test]
    fn natural_child_exit_closes_master_before_waiting_for_output_eof() -> Result<()> {
        smol::block_on(async {
            let (terminal, shared, _) = fixture();
            let (_commands, receiver) = smol::channel::unbounded();
            let (events, output) = smol::channel::unbounded();
            let task = smol::spawn(run_local_terminal(terminal, receiver, events));
            // The reader cannot reach EOF until master close. The child waiter
            // must independently notice normal exit and start that close.
            lock(&shared).exited = true;
            shared.changed.notify_all();
            finish(task, &output, &shared, false).await?;
            ensure!(lock(&shared).master_closed);
            Ok(())
        })
    }
}
