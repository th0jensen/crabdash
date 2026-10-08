use anyhow::{Result, ensure};
use std::{
    ffi::CStr,
    fs::{self, File, OpenOptions},
    os::unix::{
        ffi::OsStringExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

fn uid() -> u32 {
    // SAFETY: getuid has no arguments or owned resources.
    unsafe { libc::getuid() }
}

pub(super) fn directory() -> Result<PathBuf> {
    // Stable across CLI, Finder, login services and changed configuration paths.
    Ok(user_home()?.join(".crabdash-instance"))
}

fn user_home() -> Result<PathBuf> {
    let mut length = 64 * 1024;
    loop {
        let mut buffer = vec![0_u8; length];
        let mut record = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        // SAFETY: all output storage is writable and remains alive until the
        // record's home directory has been copied; libc owns no returned memory.
        let status = unsafe {
            libc::getpwuid_r(
                uid(),
                record.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && length < 1024 * 1024 {
            length *= 2;
            continue;
        }
        ensure!(
            status == 0 && !result.is_null(),
            "Unable to locate your native user home directory"
        );
        // SAFETY: a successful lookup with a nonnull result initialized record.
        let record = unsafe { record.assume_init() };
        ensure!(
            !record.pw_dir.is_null(),
            "Missing native user home directory"
        );
        // SAFETY: getpwuid_r returns a null-terminated pw_dir in its live buffer.
        let home = unsafe { CStr::from_ptr(record.pw_dir) };
        let path = PathBuf::from(std::ffi::OsString::from_vec(home.to_bytes().to_vec()));
        ensure!(path.is_absolute(), "Invalid native user home directory");
        return Ok(path);
    }
}

pub(super) fn prepare_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == uid()
            && metadata.permissions().mode() & 0o077 == 0,
        "Crabdash instance directory must be a private directory owned by your user"
    );
    Ok(())
}

pub(super) fn file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.mode(0o600);
    options.custom_flags(libc::O_NOFOLLOW);
    options
}

pub(super) fn validate_file(file: &File) -> Result<()> {
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.uid() == uid() && metadata.permissions().mode() & 0o077 == 0,
        "Crabdash instance files must be private files owned by your user"
    );
    Ok(())
}
