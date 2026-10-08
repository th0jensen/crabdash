//! Command ownership and cancellation. Only resource collection opts into a
//! deadline; ordinary actions retain their existing unbounded transport.
use anyhow::{Context as _, Result, anyhow, bail};
use smol::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    process::Command,
};
use std::{
    future::Future,
    pin::Pin,
    process::Stdio,
    task::{Context, Poll},
    time::Instant,
};
use tokio::task::{JoinError, JoinHandle};
use utils::{args::Args, output::Output};

pub(crate) fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        bail!("Command deadline exceeded");
    }
    Ok(())
}

fn local_command(program: &str, args: &Args) -> Command {
    let mut command = std::process::Command::new(program);
    command.args(args);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    Command::from(command)
}

fn output(program: &str, args: &Args, result: std::process::Output) -> Result<Output> {
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
        let message = if !stderr.is_empty() {
            stderr
        } else {
            format!("{program} exited with status {}", result.status)
        };
        tracing::error!(
            cmd = %program,
            args = ?args,
            status = %result.status,
            stderr = %String::from_utf8_lossy(&result.stderr).trim(),
            stdout = %String::from_utf8_lossy(&result.stdout).trim(),
            "Local command failed"
        );
        return Err(anyhow!(message));
    }
    Ok(Output::from(result.stdout))
}

pub(crate) async fn run(program: &str, args: &Args) -> Result<Output> {
    output(program, args, local_command(program, args).output().await?)
}

pub(crate) async fn run_until(program: &str, args: &Args, deadline: Instant) -> Result<Output> {
    run_with_stdin_until(program, args, None, deadline).await
}

pub(crate) async fn run_with_input_until(
    program: &str,
    args: &Args,
    input: &[u8],
    deadline: Instant,
) -> Result<Output> {
    run_with_stdin_until(program, args, Some(input), deadline).await
}

async fn run_with_stdin_until(
    program: &str,
    args: &Args,
    input: Option<&[u8]>,
    deadline: Instant,
) -> Result<Output> {
    check_deadline(deadline)?;
    let mut child = local_command(program, args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Also covers external cancellation of the whole collection future.
        // async-process's default reap_on_drop keeps abandoned waits owned.
        .kill_on_drop(true)
        .spawn()?;
    let stdin = if input.is_some() {
        Some(child.stdin.take().context("Missing command stdin pipe")?)
    } else {
        None
    };
    let mut stdout = child.stdout.take().context("Missing command stdout pipe")?;
    let mut stderr = child.stderr.take().context("Missing command stderr pipe")?;
    let result = smol::future::race(
        async {
            let (written, ((stdout, stderr), status)) = smol::future::zip(
                async {
                    if let (Some(bytes), Some(mut stdin)) = (input, stdin) {
                        stdin.write_all(bytes).await?;
                        stdin.flush().await?;
                        // EOF lets noninteractive script loaders execute their
                        // complete input, without closing either output pipe.
                        drop(stdin);
                    }
                    Ok::<_, std::io::Error>(())
                },
                smol::future::zip(
                    smol::future::zip(
                        async {
                            let mut bytes = Vec::new();
                            stdout.read_to_end(&mut bytes).await?;
                            Ok::<_, std::io::Error>(bytes)
                        },
                        async {
                            let mut bytes = Vec::new();
                            stderr.read_to_end(&mut bytes).await?;
                            Ok::<_, std::io::Error>(bytes)
                        },
                    ),
                    child.status(),
                ),
            )
            .await;
            Some((
                written,
                (|| {
                    Ok::<_, std::io::Error>(std::process::Output {
                        status: status?,
                        stdout: stdout?,
                        stderr: stderr?,
                    })
                })(),
            ))
        },
        async {
            smol::Timer::at(deadline).await;
            None
        },
    )
    .await;
    match result {
        Some((written, result)) => {
            check_deadline(deadline)?;
            // Prefer the child's diagnostic on failure; a broken input pipe
            // is often just a consequence of its early exit.
            let output = output(program, args, result?)?;
            written.context("Unable to write command input")?;
            Ok(output)
        }
        None => {
            // Kill only the direct child we own; this makes no promise about
            // descendants. Reap promptly when possible, but a kernel-stuck
            // child must not turn timeout cleanup into another infinite wait.
            if let Err(error) = child.kill() {
                if child.try_status()?.is_none() {
                    return Err(error).context("Unable to kill timed-out command");
                }
            }
            smol::future::race(
                async {
                    let _ = child.status().await;
                },
                async {
                    smol::Timer::after(std::time::Duration::from_secs(1)).await;
                },
            )
            .await;
            bail!("Command deadline exceeded: {program}")
        }
    }
}

/// Tokio JoinHandle otherwise detaches on drop. The task must remain owned by
/// its caller so cancelling a collector also drops its SSH guard and channel.
pub(crate) struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> AbortOnDrop<T> {
    pub fn new(task: JoinHandle<T>) -> Self {
        Self(task)
    }
}
impl<T> Future for AbortOnDrop<T> {
    type Output = std::result::Result<T, JoinError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}
impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf, time::Duration};
    use utils::args;
    use uuid::Uuid;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("crabdash-command-{}", Uuid::new_v4())))
        }
        fn args(&self) -> Args {
            args![
                "-c",
                "printf '%s' \"$$\" > \"$1\"; exec sleep 10",
                "fixture",
                self.0.to_string_lossy().into_owned()
            ]
        }
        async fn wait_for_pid(&self) -> Result<u32> {
            let until = Instant::now() + Duration::from_secs(2);
            loop {
                if let Ok(pid) = fs::read_to_string(&self.0) {
                    if let Ok(pid) = pid.parse() {
                        return Ok(pid);
                    }
                }
                ensure_before(until)?;
                smol::Timer::after(Duration::from_millis(10)).await;
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    fn ensure_before(deadline: Instant) -> Result<()> {
        if Instant::now() >= deadline {
            bail!("Fixture cleanup deadline exceeded");
        }
        Ok(())
    }
    async fn assert_reaped(pid: u32) -> Result<()> {
        let path = PathBuf::from(format!("/proc/{pid}"));
        let deadline = Instant::now() + Duration::from_secs(2);
        while path.exists() {
            ensure_before(deadline)?;
            smol::Timer::after(Duration::from_millis(10)).await;
        }
        Ok(())
    }

    #[test]
    fn local_deadline_kills_reaps_and_allows_the_next_command() -> Result<()> {
        smol::block_on(async {
            let fixture = Fixture::new();
            let result = run_until(
                "sh",
                &fixture.args(),
                Instant::now() + Duration::from_secs(1),
            )
            .await;
            assert!(result.is_err());
            assert_reaped(fixture.wait_for_pid().await?).await?;
            let output = run_until(
                "sh",
                &args!["-c", "printf recovered"],
                Instant::now() + Duration::from_secs(2),
            )
            .await?;
            assert_eq!(output.as_ref(), b"recovered");
            Ok(())
        })
    }

    #[test]
    fn cancellation_drops_the_owned_local_child_instead_of_detaching_it() -> Result<()> {
        smol::block_on(async {
            let fixture = Fixture::new();
            let args = fixture.args();
            let pid = smol::future::race(
                async {
                    run_until("sh", &args, Instant::now() + Duration::from_secs(10)).await?;
                    bail!("Fixture unexpectedly completed")
                },
                fixture.wait_for_pid(),
            )
            .await?;
            assert_reaped(pid).await
        })
    }

    #[test]
    fn success_before_deadline_preserves_output_and_exit_errors() -> Result<()> {
        smol::block_on(async {
            let until = Instant::now() + Duration::from_secs(2);
            assert_eq!(
                run_until("sh", &args!["-c", "printf '雪'; printf warning >&2"], until)
                    .await?
                    .as_ref(),
                "雪".as_bytes()
            );
            let result = run_until("sh", &args!["-c", "printf denied >&2; exit 7"], until).await;
            assert_eq!(
                result.err().map(|error| error.to_string()),
                Some("denied".into())
            );
            Ok(())
        })
    }

    #[test]
    fn exhausted_budget_does_not_spawn_a_child() -> Result<()> {
        smol::block_on(async {
            let fixture = Fixture::new();
            assert!(
                run_until("sh", &fixture.args(), Instant::now())
                    .await
                    .is_err()
            );
            assert!(!fixture.0.exists());
            Ok(())
        })
    }

    #[test]
    fn input_and_both_output_pipes_progress_together_and_deliver_eof() -> Result<()> {
        smol::block_on(async {
            let input = "雪 ‘literal’\n".repeat(30_000).into_bytes();
            let script = "import sys; sys.stdout.buffer.write(b'o'*200000); sys.stdout.flush(); sys.stderr.buffer.write(b'e'*200000); sys.stderr.flush(); sys.stdout.buffer.write(sys.stdin.buffer.read())";
            let output = run_with_input_until(
                "python3",
                &args!["-c", script],
                &input,
                Instant::now() + Duration::from_secs(5),
            )
            .await?;
            assert_eq!(output.as_ref()[..200_000], vec![b'o'; 200_000]);
            assert_eq!(&output.as_ref()[200_000..], input);
            let output = run_with_input_until(
                "sh",
                &args!["-c", "cat; printf eof"],
                &[],
                Instant::now() + Duration::from_secs(2),
            )
            .await?;
            assert_eq!(output.as_ref(), b"eof");
            Ok(())
        })
    }

    #[test]
    fn input_backpressure_cannot_outlive_its_deadline_or_cancellation() -> Result<()> {
        smol::block_on(async {
            let input = vec![b'x'; 512 * 1024];
            let fixture = Fixture::new();
            let result = run_with_input_until(
                "sh",
                &fixture.args(),
                &input,
                Instant::now() + Duration::from_millis(700),
            )
            .await;
            assert!(result.is_err());
            assert_reaped(fixture.wait_for_pid().await?).await?;

            let cancelled = Fixture::new();
            let args = cancelled.args();
            let pid = smol::future::race(
                async {
                    run_with_input_until(
                        "sh",
                        &args,
                        &input,
                        Instant::now() + Duration::from_secs(10),
                    )
                    .await?;
                    bail!("Input fixture unexpectedly completed")
                },
                cancelled.wait_for_pid(),
            )
            .await?;
            assert_reaped(pid).await
        })
    }

    #[test]
    fn early_failure_with_large_input_preserves_the_child_diagnostic() -> Result<()> {
        smol::block_on(async {
            let result = run_with_input_until(
                "sh",
                &args!["-c", "printf denied >&2; exit 7"],
                &vec![b'x'; 512 * 1024],
                Instant::now() + Duration::from_secs(2),
            )
            .await;
            assert_eq!(
                result.err().map(|error| error.to_string()),
                Some("denied".into())
            );
            Ok(())
        })
    }
}
