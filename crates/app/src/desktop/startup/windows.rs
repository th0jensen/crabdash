//! Windows login startup is a user-scoped Run registry value.
use anyhow::{Context as _, Result, bail};
use std::{env, mem, ptr};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
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

pub(super) fn startup_enabled() -> Result<bool> {
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
        ERROR_FILE_NOT_FOUND => return Ok(false),
        ERROR_SUCCESS => {}
        _ => bail!(
            "Unable to read Windows login preferences: {}",
            std::io::Error::from_raw_os_error(opened as i32)
        ),
    }
    let key = Key(handle);
    let result = unsafe {
        RegGetValueW(
            key.0,
            ptr::null(),
            wide("Crabdash").as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    match result {
        ERROR_SUCCESS => Ok(true),
        ERROR_FILE_NOT_FOUND => Ok(false),
        _ => bail!(
            "Unable to read Windows login startup: {}",
            std::io::Error::from_raw_os_error(result as i32)
        ),
    }
}

pub(super) fn set_login_startup(enabled: bool) -> Result<()> {
    let key = writable_key()?;
    let name = wide("Crabdash");
    let result = if enabled {
        let executable = env::current_exe().context("Unable to locate Crabdash")?;
        let command = format!(
            "\"{}\"",
            executable
                .to_str()
                .context("Crabdash's path is not valid Unicode")?
        );
        if command.encode_utf16().count() > 260 {
            bail!(
                "The Crabdash path is too long for Windows login startup; move it to a shorter installation path"
            );
        }
        let command = wide(&command);
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

pub(super) fn startup_warning() -> Result<Option<String>> {
    Ok(None)
}
