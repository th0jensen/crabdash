use anyhow::{Result, ensure};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::UI::Shell::{CSIDL_LOCAL_APPDATA, SHGFP_TYPE_CURRENT, SHGetFolderPathW};

pub(super) fn directory() -> Result<PathBuf> {
    let mut path = [0_u16; 260];
    // Resolve the actual user profile, independently of process environment or
    // executable location. The profile directory supplies its inherited ACL.
    let result = unsafe {
        SHGetFolderPathW(
            ptr::null_mut(),
            CSIDL_LOCAL_APPDATA as i32,
            ptr::null_mut(),
            SHGFP_TYPE_CURRENT as u32,
            path.as_mut_ptr(),
        )
    };
    ensure!(
        result >= 0,
        "Unable to locate your Windows application data directory"
    );
    let length = path
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(path.len());
    Ok(PathBuf::from(OsString::from_wide(&path[..length])).join("Crabdash/instance"))
}

pub(super) fn prepare_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Invalid Crabdash instance directory"
    );
    Ok(())
}

pub(super) fn file_options() -> OpenOptions {
    OpenOptions::new()
}
pub(super) fn validate_file(file: &File) -> Result<()> {
    ensure!(file.metadata()?.is_file(), "Invalid Crabdash instance file");
    Ok(())
}
