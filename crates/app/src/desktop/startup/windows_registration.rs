//! Bounded validation of the user-owned Windows Run value, without native APIs.
use anyhow::{Result, bail};

pub(super) const CAPACITY: usize = 261;
const REG_SZ: u32 = 1;
const INVALID_ENTRY: &str =
    "The Windows login startup entry is invalid. Turn it off and on to repair it.";
const DIFFERENT_COMMAND: &str =
    "The Windows login startup entry uses a different command. Turn it off and on to update it.";

pub(super) enum Registration<'a> {
    Missing,
    TooLarge,
    Value {
        value_type: u32,
        bytes: u32,
        buffer: &'a [u16],
    },
}

/// The Run command limit counts UTF-16 units, excluding the terminator.
pub(super) fn quoted_command(path: &str) -> Result<String> {
    if path.is_empty() || path.contains(['\0', '"']) {
        bail!("Crabdash's path cannot be used for Windows login startup");
    }
    let command = format!("\"{path}\"");
    if command.encode_utf16().count() >= CAPACITY {
        bail!(
            "The Crabdash path is too long for Windows login startup; move it to a shorter installation path"
        );
    }
    Ok(command)
}

fn decode(value_type: u32, bytes: u32, buffer: &[u16]) -> Option<String> {
    let bytes = bytes as usize;
    if value_type != REG_SZ || bytes == 0 || bytes % 2 != 0 || bytes > CAPACITY * 2 {
        return None;
    }
    let data = buffer.get(..bytes / 2)?;
    let end = data.iter().position(|unit| *unit == 0)?;
    if end == 0 || data[end..].iter().any(|unit| *unit != 0) {
        return None;
    }
    String::from_utf16(&data[..end]).ok()
}

pub(super) fn status(
    registration: Registration<'_>,
    expected: impl FnOnce() -> Result<String>,
) -> (bool, Option<String>) {
    let command = match registration {
        Registration::Missing => return (false, None),
        Registration::TooLarge => None,
        Registration::Value {
            value_type,
            bytes,
            buffer,
        } => decode(value_type, bytes, buffer),
    };
    let Some(command) = command else {
        return (true, Some(INVALID_ENTRY.to_owned()));
    };
    let warning = match expected() {
        Ok(expected) if expected == command => None,
        Ok(_) => Some(DIFFERENT_COMMAND.to_owned()),
        Err(error) => Some(format!("Unable to verify Windows login startup: {error}")),
    };
    (true, warning)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit(command: &str, expected_path: &str) -> (bool, Option<String>) {
        let buffer: Vec<_> = command.encode_utf16().chain(Some(0)).collect();
        status(
            Registration::Value {
                value_type: REG_SZ,
                bytes: (buffer.len() * 2) as u32,
                buffer: &buffer,
            },
            || quoted_command(expected_path),
        )
    }

    #[test]
    fn missing_value_does_not_need_an_executable_and_valid_unicode_command_matches() -> Result<()> {
        assert_eq!(
            status(Registration::Missing, || bail!("missing executable")),
            (false, None)
        );
        let path = r"C:\Program Files\Crabdash 🦀\crabdash.exe";
        let command = quoted_command(path)?;
        assert_eq!(audit(&command, path), (true, None));
        Ok(())
    }

    #[test]
    fn stale_path_and_unexpected_arguments_remain_enabled_with_different_command_warning()
    -> Result<()> {
        let path = r"C:\Crabdash\crabdash.exe";
        for command in [
            r#""C:\Old\crabdash.exe""#.to_owned(),
            format!("{} --unexpected", quoted_command(path)?),
        ] {
            assert_eq!(
                audit(&command, path),
                (true, Some(DIFFERENT_COMMAND.to_owned()))
            );
        }
        Ok(())
    }

    #[test]
    fn malformed_present_values_remain_enabled_and_can_be_removed() {
        for (value_type, bytes, buffer) in [
            (REG_SZ, 0, vec![]),
            (REG_SZ, 2, vec![0]),
            (2, 4, vec![65, 0]),
            (REG_SZ, 3, vec![65, 0]),
            (REG_SZ, 6, vec![65, 0]),
            (REG_SZ, 524, vec![0; CAPACITY]),
            (REG_SZ, 2, vec![65]),
            (REG_SZ, 6, vec![65, 0, 66]),
            (REG_SZ, 4, vec![0xd800, 0]),
        ] {
            assert_eq!(
                status(
                    Registration::Value {
                        value_type,
                        bytes,
                        buffer: &buffer
                    },
                    || bail!("must not decode malformed data")
                ),
                (true, Some(INVALID_ENTRY.to_owned()))
            );
        }
        assert_eq!(
            status(Registration::TooLarge, || bail!(
                "must not inspect an oversized buffer"
            )),
            (true, Some(INVALID_ENTRY.to_owned()))
        );
    }

    #[test]
    fn returned_length_limits_decoding_and_zero_padding_is_valid() -> Result<()> {
        let path = r"C:\crabdash.exe";
        let command = quoted_command(path)?;
        let mut buffer: Vec<_> = command.encode_utf16().chain(Some(0)).collect();
        let bytes = (buffer.len() * 2) as u32;
        buffer.push(65);
        assert_eq!(
            status(
                Registration::Value {
                    value_type: REG_SZ,
                    bytes,
                    buffer: &buffer
                },
                || quoted_command(path)
            ),
            (true, None)
        );
        buffer.pop();
        buffer.push(0);
        assert_eq!(
            status(
                Registration::Value {
                    value_type: REG_SZ,
                    bytes: bytes + 2,
                    buffer: &buffer
                },
                || quoted_command(path)
            ),
            (true, None)
        );
        Ok(())
    }

    #[test]
    fn quoted_command_uses_utf16_limit_including_quotes_but_excluding_nul() -> Result<()> {
        let path = "a".repeat(258);
        let command = quoted_command(&path)?;
        assert_eq!(command.encode_utf16().count(), 260);
        assert_eq!(audit(&command, &path), (true, None));
        let non_bmp_path = format!("{}🦀", "a".repeat(256));
        let non_bmp_command = quoted_command(&non_bmp_path)?;
        assert_eq!(non_bmp_command.encode_utf16().count(), 260);
        assert_eq!(audit(&non_bmp_command, &non_bmp_path), (true, None));
        assert!(quoted_command(&"a".repeat(259)).is_err());
        assert!(quoted_command(&format!("{}🦀", "a".repeat(257))).is_err());
        assert!(quoted_command("").is_err());
        assert!(quoted_command("bad\0path").is_err());
        Ok(())
    }

    #[test]
    fn executable_lookup_failure_preserves_present_registration() {
        let buffer = [65, 0];
        let (enabled, warning) = status(
            Registration::Value {
                value_type: REG_SZ,
                bytes: 4,
                buffer: &buffer,
            },
            || bail!("Unable to locate Crabdash"),
        );
        assert!(enabled);
        assert!(warning.is_some_and(|warning| warning.contains("Unable to locate Crabdash")));
    }
}
