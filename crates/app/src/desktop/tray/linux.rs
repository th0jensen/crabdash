//! StatusNotifierItem integration for Linux desktops, independent of GPUI's UI thread.
use gpui::{App, Global, Window};
use ksni::TrayMethods;
use smol::channel::{Receiver, Sender};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

const CONNECTING: u8 = 0;
const AVAILABLE: u8 = 1;
const OFFLINE: u8 = 2;

#[derive(Clone, Default)]
struct TrayState(Arc<AtomicU8>);
impl Global for TrayState {}

use super::TrayCommand;
struct CrabdashTray {
    commands: Sender<TrayCommand>,
    available: Arc<AtomicU8>,
    icon: Vec<ksni::Icon>,
    activation_token: Option<String>,
}

impl CrabdashTray {
    fn registered(&self) {
        // ksni starts its watcher task before returning the handle. An offline
        // callback may already have arrived; never overwrite it with success.
        let _ = self.available.compare_exchange(
            CONNECTING,
            AVAILABLE,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn show(&mut self) {
        let token = self.activation_token.take();
        self.send(TrayCommand::Show(token));
    }

    fn preferences(&mut self) {
        let token = self.activation_token.take();
        self.send(TrayCommand::Preferences(token));
    }

    fn send(&self, command: TrayCommand) {
        let _ = self.commands.try_send(command);
    }
}

impl ksni::Tray for CrabdashTray {
    fn id(&self) -> String {
        "crabdash".into()
    }
    fn title(&self) -> String {
        "Crabdash".into()
    }
    fn icon_name(&self) -> String {
        "crabdash".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }
    fn activate(&mut self, _: i32, _: i32) {
        self.show();
    }
    fn provide_xdg_activation_token(&mut self, token: String) {
        self.activation_token = Some(token);
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Crabdash".into(),
            description: "Running — click to open".into(),
            ..Default::default()
        }
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            ksni::menu::StandardItem::<Self> {
                label: "Show Crabdash".into(),
                activate: Box::new(|tray| tray.show()),
                ..Default::default()
            }
            .into(),
            ksni::menu::StandardItem::<Self> {
                label: "Preferences…".into(),
                activate: Box::new(|tray| tray.preferences()),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            ksni::menu::StandardItem::<Self> {
                label: "Quit".into(),
                activate: Box::new(|tray| tray.send(TrayCommand::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
    fn watcher_online(&self) {
        self.available.store(AVAILABLE, Ordering::Release);
    }
    fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
        self.available.store(OFFLINE, Ordering::Release);
        // A disappearing panel must never leave the app inaccessible.
        self.send(TrayCommand::Show(None));
        true
    }
}

fn icon() -> Vec<ksni::Icon> {
    let Ok(image) = image::load_from_memory_with_format(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/icons/AppIcon.png"
        )),
        image::ImageFormat::Png,
    ) else {
        return Vec::new();
    };
    let image = image
        .resize_exact(32, 32, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let mut data = image.into_vec();
    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }
    vec![ksni::Icon {
        width: 32,
        height: 32,
        data,
    }]
}

pub(crate) fn start(cx: &mut App) -> Option<Receiver<TrayCommand>> {
    let state = TrayState::default();
    cx.set_global(state.clone());
    let (commands, receiver) = smol::channel::unbounded();
    smol::spawn(async move {
        let icon = icon();
        let mut warned = false;
        loop {
            state.0.store(CONNECTING, Ordering::Release);
            let tray = CrabdashTray { commands: commands.clone(), available: state.0.clone(), icon: icon.clone(), activation_token: None };
            match tray.spawn().await {
                Ok(handle) => {
                    let _ = handle.update(|tray| tray.registered()).await;
                    while !handle.is_closed() {
                        smol::Timer::after(std::time::Duration::from_secs(2)).await;
                    }
                    state.0.store(OFFLINE, Ordering::Release);
                    let _ = commands.try_send(TrayCommand::Show(None));
                }
                Err(error) => {
                    state.0.store(OFFLINE, Ordering::Release);
                    if !warned {
                        tracing::warn!(%error, "Desktop tray unavailable; window close will quit until tray support is available");
                        warned = true;
                    }
                }
            }
            // Login startup can precede the desktop's tray host.
            smol::Timer::after(std::time::Duration::from_secs(3)).await;
        }
    }).detach();
    Some(receiver)
}

/// Keep the window and its remote sessions alive only when the tray can restore it.
pub(crate) fn should_close(window: &mut Window, cx: &mut App) -> bool {
    if crate::features::preferences::current(cx).close_to_tray
        && cx
            .try_global::<TrayState>()
            .is_some_and(|state| state.0.load(Ordering::Acquire) == AVAILABLE)
    {
        crate::desktop::window::hide_to_tray(window, cx);
        false
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_icon_is_valid_argb() {
        let icons = icon();
        assert_eq!(icons.len(), 1);
        assert_eq!((icons[0].width, icons[0].height), (32, 32));
        assert_eq!(icons[0].data.len(), 32 * 32 * 4);
    }
    #[test]
    fn tray_menu_and_activation_request_ui_commands() {
        use ksni::Tray;
        let (commands, receiver) = smol::channel::unbounded();
        let mut tray = CrabdashTray {
            commands,
            available: Arc::default(),
            icon: vec![],
            activation_token: None,
        };
        tray.activate(0, 0);
        assert!(matches!(receiver.try_recv(), Ok(TrayCommand::Show(None))));
        tray.provide_xdg_activation_token("compositor-token".into());
        let menu = tray.menu();
        let ksni::MenuItem::Standard(show) = &menu[0] else {
            panic!("expected show item");
        };
        (show.activate)(&mut tray);
        assert!(
            matches!(receiver.try_recv(), Ok(TrayCommand::Show(Some(token))) if token == "compositor-token")
        );
        tray.activate(0, 0);
        assert!(matches!(receiver.try_recv(), Ok(TrayCommand::Show(None))));
        tray.provide_xdg_activation_token("preferences-token".into());
        let menu = tray.menu();
        let ksni::MenuItem::Standard(preferences) = &menu[1] else {
            panic!("expected preferences item");
        };
        (preferences.activate)(&mut tray);
        assert!(
            matches!(receiver.try_recv(), Ok(TrayCommand::Preferences(Some(token))) if token == "preferences-token")
        );
        tray.activate(0, 0);
        assert!(matches!(receiver.try_recv(), Ok(TrayCommand::Show(None))));
        (preferences.activate)(&mut tray);
        assert!(matches!(
            receiver.try_recv(),
            Ok(TrayCommand::Preferences(None))
        ));
        let ksni::MenuItem::Standard(quit) = &menu[3] else {
            panic!("expected quit item");
        };
        (quit.activate)(&mut tray);
        assert!(matches!(receiver.try_recv(), Ok(TrayCommand::Quit)));
        tray.watcher_online();
        assert_eq!(tray.available.load(Ordering::Acquire), AVAILABLE);
        tray.watcher_offline(ksni::OfflineReason::No);
        assert_eq!(tray.available.load(Ordering::Acquire), OFFLINE);
        assert!(matches!(receiver.try_recv(), Ok(TrayCommand::Show(None))));
    }

    #[test]
    fn initial_registration_preserves_a_concurrent_host_loss() {
        use ksni::Tray;
        let (commands, receiver) = smol::channel::unbounded();
        let tray = CrabdashTray {
            commands,
            available: Arc::default(),
            icon: vec![],
            activation_token: None,
        };
        tray.registered();
        assert_eq!(tray.available.load(Ordering::Acquire), AVAILABLE);
        tray.watcher_offline(ksni::OfflineReason::No);
        tray.registered();
        assert_eq!(tray.available.load(Ordering::Acquire), OFFLINE);
        assert!(matches!(receiver.try_recv(), Ok(TrayCommand::Show(None))));
        tray.watcher_online();
        assert_eq!(tray.available.load(Ordering::Acquire), AVAILABLE);
    }
}

pub(super) const SUPPORTED: bool = true;
