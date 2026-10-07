//! Windows login startup is a user-scoped Run registry value.
use super::windows_registration::{self, CAPACITY, Registration, quoted_command};
use anyhow::{Context as _, Result, bail};
use std::{env, mem, ptr};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS},
    System::Registry::*,
};

pub(super) const SUPPORTED: bool = true;
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

fn writable_key() -> Result<Key> {
    let mut key = ptr::null_mut();
    // SAFETY: All UTF-16 arguments are terminated and the returned handle is
    // owned and closed by Key. This never modifies system-wide registration.
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            ptr::null(),
            0,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            ptr::null(),
            &mut key,
            ptr::null_mut(),
        )
    };
    if result != ERROR_SUCCESS {
        bail!(
            "Unable to open Windows login preferences: {}",
            std::io::Error::from_raw_os_error(result as i32)
        );
    }
    Ok(Key(key))
}

pub(super) fn startup_status() -> Result<(bool, Option<String>)> {
    // Reading status never creates registry entries.
    let mut handle = ptr::null_mut();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut handle,
        )
    };
    match opened {
        ERROR_FILE_NOT_FOUND => return Ok((false, None)),
        ERROR_SUCCESS => {}
        _ => bail!(
            "Unable to read Windows login preferences: {}",
            std::io::Error::from_raw_os_error(opened as i32)
        ),
    }
    let key = Key(handle);
    let mut buffer = [0u16; CAPACITY];
    let mut value_type = 0;
    let mut bytes = (buffer.len() * mem::size_of::<u16>()) as u32;
    // SAFETY: The single read is bounded by this fixed buffer's byte size.
    // RegGetValueW reports its type and returned size; oversized/failed reads
    // are never decoded. The handle is user-scoped and remains owned by Key.
    let result = unsafe {
        RegGetValueW(
            key.0,
            ptr::null(),
            wide("Crabdash").as_ptr(),
            RRF_RT_ANY | RRF_NOEXPAND,
            &mut value_type,
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    match result {
        ERROR_SUCCESS => Ok(windows_registration::status(
            Registration::Value {
                value_type,
                bytes,
                buffer: &buffer,
            },
            expected_command,
        )),
        ERROR_MORE_DATA => Ok(windows_registration::status(
            Registration::TooLarge,
            expected_command,
        )),
        ERROR_FILE_NOT_FOUND => Ok(windows_registration::status(
            Registration::Missing,
            expected_command,
        )),
        _ => bail!(
            "Unable to read Windows login startup: {}",
            std::io::Error::from_raw_os_error(result as i32)
        ),
    }
}

fn expected_command() -> Result<String> {
    let executable = env::current_exe().context("Unable to locate Crabdash")?;
    quoted_command(
        executable
            .to_str()
            .context("Crabdash's path is not valid Unicode")?,
    )
}

pub(super) fn set_login_startup(enabled: bool) -> Result<()> {
    let key = writable_key()?;
    let name = wide("Crabdash");
    let result = if enabled {
        let command = wide(&expected_command()?);
        unsafe {
            RegSetValueExW(
                key.0,
                name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * mem::size_of::<u16>()) as u32,
            )
        }
    } else {
        unsafe { RegDeleteValueW(key.0, name.as_ptr()) }
    };
    if result == ERROR_SUCCESS || (!enabled && result == ERROR_FILE_NOT_FOUND) {
        return Ok(());
    }
    bail!(
        "Unable to change Windows login startup: {}",
        std::io::Error::from_raw_os_error(result as i32)
    )
}
