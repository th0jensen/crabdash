//! ConPTY has no tcgetpgrp. Follow the shell's live child chain using the native
//! process snapshot; console host helpers are excluded from the visible name.
use std::{ffi::c_void, mem::size_of};
type Handle = *mut c_void;
#[repr(C)]
struct Entry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    priority: i32,
    flags: u32,
    exe: [u16; 260],
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
    fn Process32FirstW(snapshot: Handle, entry: *mut Entry) -> i32;
    fn Process32NextW(snapshot: Handle, entry: *mut Entry) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}
struct Snapshot(Handle);
impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: the snapshot owns one valid kernel handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub(in crate::terminal) fn foreground(pid: Option<u32>) -> Option<String> {
    let pid = pid?;
    // SAFETY: process enumeration needs no buffers or borrowed handles.
    let raw = unsafe { CreateToolhelp32Snapshot(2, 0) };
    if raw == (-1isize as Handle) || raw.is_null() {
        return None;
    }
    let snapshot = Snapshot(raw);
    // SAFETY: this Win32 POD structure accepts an all-zero initialization.
    let mut entry: Entry = unsafe { std::mem::zeroed() };
    entry.size = size_of::<Entry>() as u32;
    let mut processes = Vec::new();
    // SAFETY: entry has the required size and remains valid throughout enumeration.
    let mut present = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while present {
        let length = entry
            .exe
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(entry.exe.len());
        if let Some(name) = super::name(&String::from_utf16_lossy(&entry.exe[..length])) {
            processes.push((entry.pid, entry.parent, name));
        }
        // SAFETY: same valid snapshot and entry buffer as above.
        present = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    let mut current = pid;
    let mut name = processes.iter().find(|entry| entry.0 == pid)?.2.clone();
    // Bound traversal even if a snapshot contains recycled/cyclic PID ancestry.
    for _ in 0..32 {
        let Some(child) = processes
            .iter()
            .filter(|entry| {
                entry.1 == current
                    && entry.0 != current
                    && !matches!(
                        entry.2.to_ascii_lowercase().as_str(),
                        "conhost" | "openconsole"
                    )
            })
            .max_by_key(|entry| entry.0)
        else {
            break;
        };
        current = child.0;
        name = child.2.clone();
    }
    Some(name)
}
