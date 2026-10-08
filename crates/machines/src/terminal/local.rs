use super::{TerminalController, TerminalOptions, TerminalSession, TerminalSize};
use anyhow::{Context as _, Result};
use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, native_pty_system};
use std::io;
#[cfg(not(target_os = "windows"))]
mod unix;
#[cfg(any(target_os = "windows", test))]
mod windows;
#[cfg(not(target_os = "windows"))]
use unix::run_local_terminal;
#[cfg(target_os = "windows")]
use windows::run_local_terminal;

struct LocalTerminal {
    master: Box<dyn MasterPty + Send>,
    reader: Box<dyn io::Read + Send>,
    writer: Box<dyn io::Write + Send>,
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

        let mut command = shell_command();
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
            reader,
            writer,
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

#[cfg(not(target_os = "windows"))]
fn shell_command() -> CommandBuilder {
    CommandBuilder::new_default_prog()
}

#[cfg(target_os = "windows")]
fn shell_command() -> CommandBuilder {
    // Windows PowerShell is bundled with supported Windows versions. Resolve
    // it under SystemRoot so launching does not depend on an edited PATH.
    if let Some(shell) = crate::powershell::local_executable() {
        let mut command = CommandBuilder::new(shell);
        command.arg("-NoLogo");
        return command;
    }
    // portable-pty's Windows default resolves COMSPEC (normally cmd.exe).
    CommandBuilder::new_default_prog()
}
