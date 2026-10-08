//! PowerShell command transport, shared by Windows machine domains.
//!
//! EncodedCommand avoids differences between cmd.exe and Unix SSH shells.
use crate::machine::Machine;
use anyhow::{Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use utils::{args, args::Args, output::Output};

#[cfg(any(target_os = "windows", test))]
mod windows;

/// Bundled local Windows PowerShell, shared with the interactive terminal.
/// The environment and filesystem are consulted only on a Windows host.
pub(crate) fn local_executable() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let root = std::env::var("SystemRoot").ok();
        windows::bundled_executable(root.as_deref(), |path| std::path::Path::new(path).is_file())
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn remote_command(encoded: &str) -> Result<String> {
    // Every argument is generated ASCII. Leaving them unquoted works with
    // Windows OpenSSH's cmd.exe and PowerShell default shells. The target
    // resolves its own executable; a desktop SystemRoot never enters SSH.
    let command =
        format!("powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand {encoded}");
    ensure!(
        command.len() <= 8191,
        "Windows SSH command is too long for the default command shell; shorten the command or its parameters"
    );
    Ok(command)
}

pub(crate) fn literal(value: &str) -> Result<String> {
    ensure!(
        !value.contains('\0'),
        "Windows parameter contains a null character"
    );
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('\'');
    for character in value.chars() {
        literal.push(character);
        // PowerShell also accepts curly/base/reversed single quotes as string
        // delimiters. Double all of them, not only the ASCII apostrophe.
        if matches!(
            character,
            '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}'
        ) {
            literal.push(character);
        }
    }
    literal.push('\'');
    Ok(literal)
}

pub(crate) fn encoded(script: &str) -> String {
    let script = format!(
        "$ErrorActionPreference = 'Stop'; [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); try {{ {script} }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}"
    );
    STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

/// Quote one native Windows argv item, independently of PowerShell's legacy
/// native-argument binder. Empty values and backslashes before quotes survive.
fn native_argument(value: &str) -> Result<String> {
    ensure!(
        !value.contains('\0'),
        "Windows argument contains a null character"
    );
    let mut argument = String::from("\"");
    let mut slashes = 0;
    for character in value.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        let count = if character == '"' {
            slashes * 2 + 1
        } else {
            slashes
        };
        argument.extend(std::iter::repeat_n('\\', count));
        argument.push(character);
        slashes = 0;
    }
    argument.extend(std::iter::repeat_n('\\', slashes * 2));
    argument.push('"');
    Ok(argument)
}

pub(crate) fn native_script(program: &str, args: &Args) -> Result<String> {
    ensure!(
        !program.trim().is_empty(),
        "Windows executable path cannot be empty"
    );
    let arguments = args
        .iter()
        .map(|argument| native_argument(argument))
        .collect::<Result<Vec<_>>>()?
        .join(" ");
    // ProcessStartInfo bypasses cmd.exe, and also avoids PowerShell 5.1's
    // argument conversion (which otherwise drops empty arguments and quotes).
    // Read both pipes concurrently so a full stderr pipe cannot deadlock stdout.
    Ok(format!(
        r#"
        $start = [System.Diagnostics.ProcessStartInfo]::new()
        $start.FileName = {program}
        $start.Arguments = {arguments}
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        $start.StandardOutputEncoding = [System.Text.UTF8Encoding]::new($false)
        $start.StandardErrorEncoding = [System.Text.UTF8Encoding]::new($false)
        $process = [System.Diagnostics.Process]::new()
        $process.StartInfo = $start
        try {{
            if (-not $process.Start()) {{ throw 'Unable to start the Windows executable' }}
            $stdout = $process.StandardOutput.ReadToEndAsync()
            $stderr = $process.StandardError.ReadToEndAsync()
            $process.WaitForExit()
            [Console]::Out.Write($stdout.GetAwaiter().GetResult())
            [Console]::Error.Write($stderr.GetAwaiter().GetResult())
            $code = $process.ExitCode
        }} finally {{ $process.Dispose() }}
        exit $code
    "#,
        program = literal(program)?,
        arguments = literal(&arguments)?
    ))
}

pub(crate) async fn run_native(
    machine: &mut Machine,
    program: &str,
    args: &Args,
) -> Result<Output> {
    run(machine, &native_script(program, args)?).await
}

pub(crate) async fn run(machine: &mut Machine, script: &str) -> Result<Output> {
    let encoded = encoded(script);
    if let Some(remote) = machine.remote.as_mut() {
        let command = remote_command(&encoded)?;
        remote.run_ssh_command(&command, &Args::new()).await
    } else {
        let executable = local_executable().unwrap_or_else(|| "powershell.exe".into());
        machine
            .run(
                &executable,
                &args![
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-EncodedCommand",
                    &encoded
                ],
            )
            .await
    }
}

// Base64 stdin keeps the source independent of the target's input code page.
// A short encoded bootstrap also stays below cmd.exe's Windows SSH limit.
const INPUT_BOOTSTRAP: &str = "$source = [System.Text.Encoding]::UTF8.GetString([System.Convert]::FromBase64String([Console]::In.ReadToEnd())); & ([ScriptBlock]::Create($source))";

/// Resource-only variant: input delivery, execution and output share the same
/// sample deadline. Source length does not consume the remote command line.
pub(crate) async fn run_until(
    machine: &mut Machine,
    script: &str,
    deadline: std::time::Instant,
) -> Result<Output> {
    let encoded = encoded(INPUT_BOOTSTRAP);
    let input = STANDARD.encode(script.as_bytes());
    if let Some(remote) = machine.remote.as_mut() {
        let command = remote_command(&encoded)?;
        remote
            .run_ssh_command_with_input_until(&command, &Args::new(), input.as_bytes(), deadline)
            .await
    } else {
        let executable = local_executable().unwrap_or_else(|| "powershell.exe".into());
        crate::command::run_with_input_until(
            &executable,
            &args![
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &encoded
            ],
            input.as_bytes(),
            deadline,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_commands_use_the_target_executable_instead_of_local_system_root() -> Result<()> {
        let local = windows::bundled_executable(Some(r"C:\Windows with spaces"), |_| true)
            .ok_or_else(|| anyhow::anyhow!("Expected bundled Windows PowerShell"))?;
        let command = remote_command(&encoded("Write-Output 'test'"))?;
        assert!(command.starts_with("powershell.exe -NoLogo "));
        assert!(!command.contains(&local));
        assert!(remote_command(&"A".repeat(8192)).is_err());
        Ok(())
    }

    #[test]
    fn names_are_literals_and_encoded_scripts_round_trip() -> Result<()> {
        let name = "service'; $(Remove-Item C:\\*) #";
        assert_eq!(literal(name)?, "'service''; $(Remove-Item C:\\*) #'");
        assert_eq!(literal("‘’‚‛")?, "'‘‘’’‚‚‛‛'");
        assert!(literal("bad\0name").is_err());
        let bytes = STANDARD.decode(encoded("Write-Output '雪'"))?;
        let utf16 = bytes
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        let decoded = String::from_utf16(&utf16)?;
        assert!(decoded.contains("Write-Output '雪'"));
        assert!(decoded.contains("exit 1"));
        Ok(())
    }

    #[test]
    fn stdin_script_payload_is_unicode_safe_and_independent_of_command_length() -> Result<()> {
        let script = "Write-Output '雪 ‘quoted’ $literal';\n".repeat(10_000);
        assert!(encoded(&script).len() > 8191);
        let input = STANDARD.encode(script.as_bytes());
        assert!(input.is_ascii());
        assert_eq!(STANDARD.decode(input)?, script.as_bytes());
        let command = format!(
            "powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand {}",
            encoded(INPUT_BOOTSTRAP)
        );
        assert!(command.len() <= 8191);
        Ok(())
    }

    // Independent decoder following Microsoft's documented Windows argv rules.
    fn parse_native_arguments(command: &str) -> Vec<String> {
        let mut characters = command.chars().peekable();
        let mut arguments = Vec::new();
        while characters.peek().is_some() {
            while characters
                .peek()
                .is_some_and(|character| matches!(character, ' ' | '\t'))
            {
                characters.next();
            }
            if characters.peek().is_none() {
                break;
            }
            let mut argument = String::new();
            let mut quoted = false;
            while let Some(character) = characters.peek().copied() {
                if !quoted && matches!(character, ' ' | '\t') {
                    break;
                }
                match character {
                    '\\' => {
                        let mut slashes = 0;
                        while characters.peek() == Some(&'\\') {
                            characters.next();
                            slashes += 1;
                        }
                        if characters.peek() == Some(&'"') {
                            characters.next();
                            argument.extend(std::iter::repeat_n('\\', slashes / 2));
                            if slashes % 2 == 1 {
                                argument.push('"');
                            } else {
                                quoted = !quoted;
                            }
                        } else {
                            argument.extend(std::iter::repeat_n('\\', slashes));
                        }
                    }
                    '"' => {
                        characters.next();
                        quoted = !quoted;
                    }
                    _ => {
                        characters.next();
                        argument.push(character);
                    }
                }
            }
            arguments.push(argument);
        }
        arguments
    }

    #[test]
    fn native_arguments_preserve_empty_quotes_unicode_and_trailing_slashes() -> Result<()> {
        let mut arguments = vec![
            "".into(),
            "MESSAGE=hello world".into(),
            "\"quoted\"".into(),
            "雪 ‘smart’ $(literal); & %PATH%".into(),
            "line\nbreak\tvalue".into(),
        ];
        for count in 0..6 {
            let slashes = "\\".repeat(count);
            arguments.extend([
                format!("C:\\folder with spaces{slashes}"),
                format!("a{slashes}\"b"),
            ]);
        }
        let command = arguments
            .iter()
            .map(|argument| native_argument(argument))
            .collect::<Result<Vec<_>>>()?
            .join(" ");
        assert_eq!(parse_native_arguments(&command), arguments);
        assert_eq!(native_argument("")?, "\"\"");
        assert!(native_argument("null\0value").is_err());
        Ok(())
    }

    #[test]
    fn native_process_script_bypasses_shells_and_propagates_cli_failure() -> Result<()> {
        let program = "C:\\Program Files\\Docker\\docker.exe";
        let script = native_script(
            program,
            &args!["run", "-e", "MESSAGE='hello'", "alpine", ""],
        )?;
        assert!(script.contains(&format!("$start.FileName = {}", literal(program)?)));
        assert!(script.contains("$start.UseShellExecute = $false"));
        assert!(script.contains("$start.Arguments ="));
        assert!(script.contains("ReadToEndAsync()"));
        assert!(script.contains("$code = $process.ExitCode"));
        assert!(script.contains("exit $code"));
        assert!(native_script("", &Args::new()).is_err());
        assert!(native_script(program, &args!["invalid\0argument"]).is_err());
        Ok(())
    }
}
