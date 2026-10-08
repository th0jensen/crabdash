//! Observe the user's Windows accent without coupling native callbacks to GPUI.
use gpui::{App, Global};
use smol::channel::Receiver;
use windows::{
    Foundation::TypedEventHandler,
    UI::ViewManagement::{UIColorType, UISettings},
    Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize},
    core::IInspectable,
};

type ColorHandler = TypedEventHandler<UISettings, IInspectable>;

/// GPUI initializes OLE as STA. Initialize WinRT with the same apartment model,
/// and balance even the successful already-initialized (S_FALSE) result.
struct Apartment;
impl Apartment {
    fn initialize() -> windows::core::Result<Self> {
        // SAFETY: start is called on the GPUI UI thread, which already owns an
        // STA. This adds a balanced WinRT initialization on that same thread.
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED)? };
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: the app-owned subscription is destroyed on its creating UI
        // thread, after its WinRT objects have been released.
        unsafe { RoUninitialize() };
    }
}

/// Declaration order releases objects before uninitializing their apartment.
/// Retaining this once per app also prevents one window from replacing another
/// window's observer or losing notifications when the first window closes.
struct Subscription {
    settings: UISettings,
    token: i64,
    _handler: ColorHandler,
    _apartment: Apartment,
}
impl Global for Subscription {}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Err(error) = self.settings.RemoveColorValuesChanged(self.token) {
            tracing::warn!("Could not unregister Windows accent observer: {error}");
        }
    }
}

fn read(settings: &UISettings) -> Option<u32> {
    settings
        .GetColorValue(UIColorType::Accent)
        .ok()
        .map(|color| (u32::from(color.R) << 16) | (u32::from(color.G) << 8) | u32::from(color.B))
}

/// Initial and changed colours are delivered to the shared accent runtime,
/// which owns deduplication and UI invalidation on the GPUI thread. None means
/// the caller should use its normal palette. Failed setup remains retryable.
pub(super) fn start(cx: &mut App) -> Option<Receiver<Option<u32>>> {
    if cx.has_global::<Subscription>() {
        return None;
    }
    let apartment = match Apartment::initialize() {
        Ok(apartment) => apartment,
        Err(error) => {
            tracing::warn!("Could not initialize Windows accent observer: {error}");
            return None;
        }
    };
    let settings = match UISettings::new() {
        Ok(settings) => settings,
        Err(error) => {
            tracing::warn!("Could not read Windows UI settings: {error}");
            return None;
        }
    };
    let (sender, receiver) = smol::channel::unbounded();
    let changed = sender.clone();
    let handler = ColorHandler::new(move |settings, _| {
        // WinRT may dispatch on another thread. Only send data here; never
        // capture a GPUI App, Window, entity, or an app-owned mutable borrow.
        let color = settings.as_ref().and_then(read);
        let _ = changed.try_send(color);
        Ok(())
    });
    let token = match settings.ColorValuesChanged(&handler) {
        Ok(token) => token,
        Err(error) => {
            tracing::warn!("Could not subscribe to Windows accent changes: {error}");
            return None;
        }
    };
    // Subscribe before the initial read so changes during setup are observed.
    let _ = sender.try_send(read(&settings));
    cx.set_global(Subscription {
        settings,
        token,
        _handler: handler,
        _apartment: apartment,
    });
    Some(receiver)
}
