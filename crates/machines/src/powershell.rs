//! PowerShell command transport, shared by Windows machine domains.
//!
//! EncodedCommand avoids differences between cmd.exe and Unix SSH shells.
use crate::machine::Machine;
use anyhow::{Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use utils::{args, args::Args, output::Output};

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

pub(crate) async fn run(machine: &mut Machine, script: &str) -> Result<Output> {
    let encoded = encoded(script);
    if let Some(remote) = machine.remote.as_mut() {
        // Every argument here is generated ASCII. Leaving them unquoted works
        // with Windows OpenSSH's cmd.exe and PowerShell default shells, and
        // avoids applying POSIX single-quote rules to a Windows shell.
        remote
            .run_ssh_command(
                &format!(
                    "powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand {encoded}"
                ),
                &Args::new(),
            )
            .await
    } else {
        machine
            .run(
                "powershell.exe",
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
