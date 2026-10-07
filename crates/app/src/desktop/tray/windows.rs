//! Native Windows notification icon with an independent Win32 message loop.
use super::TrayCommand;
use gpui::{App, Global, Window};
use smol::channel::{Receiver, Sender};
use std::{
    mem, ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicIsize, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*},
};

pub(super) const SUPPORTED: bool = true;
const CALLBACK: u32 = WM_APP + 1;
const KEY_SELECT: u32 = NIN_SELECT | 1;
const SHOW: usize = 1;
const PREFERENCES: usize = 2;
const QUIT: usize = 3;
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
#[derive(Clone, Default)]
struct TrayState {
    available: Arc<AtomicBool>,
    hwnd: Arc<AtomicIsize>,
    exiting: Arc<AtomicBool>,
}
impl Global for TrayState {}
struct TrayWorker(Option<std::thread::JoinHandle<()>>);
impl Global for TrayWorker {}
struct NativeTray {
    commands: Sender<TrayCommand>,
    state: TrayState,
    icon: HICON,
    restart: u32,
}

impl NativeTray {
    fn data(&self, hwnd: HWND) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: CALLBACK,
            hIcon: self.icon,
            ..Default::default()
        };
        let title = wide("Crabdash — open dashboard");
        data.szTip[..title.len()].copy_from_slice(&title);
        data
    }
    fn add(&self, hwnd: HWND) {
        let mut data = self.data(hwnd);
        let available = unsafe { Shell_NotifyIconW(NIM_ADD, &data) != 0 };
        if available {
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            if unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) } == 0 {
                unsafe {
                    Shell_NotifyIconW(NIM_DELETE, &data);
                }
                self.state.available.store(false, Ordering::Release);
                tracing::warn!("Windows could not configure the Crabdash notification icon");
                // Explorer may have restarted while the dashboard was hidden.
                // Losing the restore icon must never make the app inaccessible.
                let _ = self.commands.try_send(TrayCommand::Show(None));
                return;
            }
        }
        self.state.available.store(available, Ordering::Release);
        if !available {
            tracing::warn!(
                "Windows notification area is unavailable; closing the window will quit"
            );
            let _ = self.commands.try_send(TrayCommand::Show(None));
        }
    }
    fn menu(&self, hwnd: HWND, location: WPARAM) {
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            AppendMenuW(menu, MF_STRING, SHOW, wide("Show Crabdash").as_ptr());
            AppendMenuW(menu, MF_STRING, PREFERENCES, wide("Preferences…").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(menu, MF_STRING, QUIT, wide("Quit Crabdash").as_ptr());
            SetMenuDefaultItem(menu, SHOW as u32, 0);
            let mut point = POINT {
                x: (location as u16 as i16) as i32,
                y: ((location >> 16) as u16 as i16) as i32,
            };
            if point.x == -1 && point.y == -1 {
                GetCursorPos(&mut point);
            }
            SetForegroundWindow(hwnd);
            let selection = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                hwnd,
                ptr::null(),
            ) as usize;
            DestroyMenu(menu);
            PostMessageW(hwnd, WM_NULL, 0, 0);
            // Return keyboard focus to the notification area even when Escape
            // cancels this menu, as required by Shell_NotifyIcon's contract.
            Shell_NotifyIconW(NIM_SETFOCUS, &self.data(hwnd));
            let command = match selection {
                SHOW => Some(TrayCommand::Show(None)),
                PREFERENCES => Some(TrayCommand::Preferences),
                QUIT => Some(TrayCommand::Quit),
                _ => None,
            };
            if let Some(command) = command {
                let _ = self.commands.try_send(command);
            }
        }
    }
}

// SAFETY: Called only by this thread's Win32 loop. GWLP_USERDATA points to the
// stable Box<NativeTray> for the entire window lifetime and is cleared on destroy.
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut NativeTray;
        if let Some(tray) = state.as_ref() {
            if message == tray.restart && tray.restart != 0 {
                tray.add(hwnd);
                return 0;
            }
            match message {
                CALLBACK => {
                    match (lparam as u32) & 0xffff {
                        NIN_SELECT | KEY_SELECT => {
                            let _ = tray.commands.try_send(TrayCommand::Show(None));
                        }
                        WM_CONTEXTMENU => tray.menu(hwnd, wparam),
                        _ => {}
                    }
                    return 0;
                }
                WM_CLOSE => {
                    DestroyWindow(hwnd);
                    return 0;
                }
                WM_DESTROY => {
                    Shell_NotifyIconW(NIM_DELETE, &tray.data(hwnd));
                    tray.state.available.store(false, Ordering::Release);
                    tray.state.hwnd.store(0, Ordering::Release);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    PostQuitMessage(0);
                    return 0;
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

fn icon() -> HICON {
    // Prefer the same executable resource used by GPUI and Explorer. Without
    // LR_SHARED this handle is owned and must be destroyed on worker teardown.
    let native = unsafe {
        LoadImageW(
            GetModuleHandleW(ptr::null()),
            1usize as *const u16,
            IMAGE_ICON,
            32,
            32,
            LR_DEFAULTCOLOR,
        )
    };
    if !native.is_null() {
        return native as HICON;
    }
    // Test harnesses and library consumers may lack the binary icon resource.
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/icons/AppIcon.ico"
    ));
    // ICO directory entries identify self-contained PNG/DIB icon resources.
    for entry in bytes
        .get(6..)
        .into_iter()
        .flat_map(|entries| entries.chunks_exact(16))
        .take(u16::from_le_bytes([bytes[4], bytes[5]]) as usize)
    {
        if entry[0] != 32 || entry[1] != 32 {
            continue;
        }
        let length = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        if let Some(resource) = offset
            .checked_add(length)
            .and_then(|end| bytes.get(offset..end))
        {
            let mut resource = resource.to_vec();
            let icon = unsafe {
                CreateIconFromResourceEx(
                    resource.as_mut_ptr(),
                    length as u32,
                    1,
                    0x00030000,
                    32,
                    32,
                    LR_DEFAULTCOLOR,
                )
            };
            if !icon.is_null() {
                return icon;
            }
        }
    }
    // Owned fallback, so the same DestroyIcon cleanup applies.
    unsafe { CopyIcon(LoadIconW(ptr::null_mut(), IDI_APPLICATION)) }
}

fn run(commands: Sender<TrayCommand>, state: TrayState) {
    unsafe {
        let icon = icon();
        if icon.is_null() {
            tracing::warn!("Unable to load Crabdash's notification icon");
            return;
        }
        let instance = GetModuleHandleW(ptr::null());
        let class = wide("CrabdashNotificationHost");
        let mut tray = Box::new(NativeTray {
            commands,
            state,
            icon,
            restart: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
        });
        let definition = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        if RegisterClassW(&definition) == 0 {
            DestroyIcon(icon);
            tracing::warn!(
                "Unable to register the Crabdash tray window: {}",
                std::io::Error::last_os_error()
            );
            return;
        }
        // A hidden top-level window receives Explorer's TaskbarCreated broadcast;
        // a message-only window would miss it and lose the icon on Explorer restart.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("Crabdash notification host").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            (&mut *tray as *mut NativeTray).cast(),
        );
        if hwnd.is_null() {
            DestroyIcon(icon);
            UnregisterClassW(class.as_ptr(), instance);
            tracing::warn!(
                "Unable to create the Crabdash tray window: {}",
                std::io::Error::last_os_error()
            );
            return;
        }
        tray.state.hwnd.store(hwnd as isize, Ordering::Release);
        if tray.state.exiting.load(Ordering::Acquire) {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        } else {
            tray.add(hwnd);
        }
        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, ptr::null_mut(), 0, 0);
            if result <= 0 {
                if result < 0 {
                    tracing::warn!(
                        "Crabdash tray message loop failed: {}",
                        std::io::Error::last_os_error()
                    );
                }
                break;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if IsWindow(hwnd) != 0 {
            DestroyWindow(hwnd);
        }
        DestroyIcon(icon);
        UnregisterClassW(class.as_ptr(), instance);
        if !tray.state.exiting.load(Ordering::Acquire) {
            let _ = tray.commands.try_send(TrayCommand::Show(None));
        }
    }
}

pub(crate) fn start(cx: &mut App) -> Option<Receiver<TrayCommand>> {
    let state = TrayState::default();
    cx.set_global(state.clone());
    let (commands, receiver) = smol::channel::unbounded();
    let worker = match std::thread::Builder::new()
        .name("Crabdash notification icon".into())
        .spawn(move || run(commands, state))
    {
        Ok(worker) => worker,
        Err(error) => {
            tracing::warn!(%error, "Unable to start the native Windows tray");
            return None;
        }
    };
    cx.set_global(TrayWorker(Some(worker)));
    cx.on_app_quit(|cx| {
        let state = cx.global::<TrayState>();
        state.exiting.store(true, Ordering::Release);
        let hwnd = state.hwnd.load(Ordering::Acquire) as HWND;
        if !hwnd.is_null() {
            unsafe {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
        let worker = cx.global_mut::<TrayWorker>().0.take();
        async move {
            if let Some(worker) = worker {
                if smol::unblock(move || worker.join()).await.is_err() {
                    tracing::warn!("The Windows tray worker stopped unexpectedly");
                }
            }
        }
    })
    .detach();
    Some(receiver)
}

pub(crate) fn should_close(window: &mut Window, cx: &mut App) -> bool {
    if crate::features::preferences::current(cx).close_to_tray
        && cx
            .try_global::<TrayState>()
            .is_some_and(|state| state.available.load(Ordering::Acquire))
    {
        crate::desktop::window::hide_to_tray(window);
        return false;
    }
    true
}
