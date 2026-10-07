//! Persistent native menu bar item; callbacks cross into GPUI through a channel.
use super::TrayCommand;
use gpui::{App, Global, Window};
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, sel,
};
use objc2_app_kit::{NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use smol::channel::{Receiver, Sender};

pub(super) const SUPPORTED: bool = true;

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The target is retained
    // for the entire lifetime of the status item because NSMenuItem targets are weak.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Sender<TrayCommand>]
    struct CrabdashStatusTarget;
    unsafe impl NSObjectProtocol for CrabdashStatusTarget {}
    impl CrabdashStatusTarget {
        #[unsafe(method(showCrabdash:))]
        fn show(&self, _sender: Option<&NSObject>) { let _ = self.ivars().try_send(TrayCommand::Show(None)); }
        #[unsafe(method(openPreferences:))]
        fn preferences(&self, _sender: Option<&NSObject>) { let _ = self.ivars().try_send(TrayCommand::Preferences(None)); }
        #[unsafe(method(quitCrabdash:))]
        fn quit(&self, _sender: Option<&NSObject>) { let _ = self.ivars().try_send(TrayCommand::Quit); }
    }
);
impl CrabdashStatusTarget {
    fn new(mtm: MainThreadMarker, commands: Sender<TrayCommand>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(commands);
        // SAFETY: NSObject's init returns an initialized object of this class.
        unsafe { msg_send![super(this), init] }
    }
}

struct TrayState {
    item: Option<Retained<NSStatusItem>>,
    _target: Retained<CrabdashStatusTarget>,
}
impl Global for TrayState {}

pub(crate) fn start(cx: &mut App) -> Option<Receiver<TrayCommand>> {
    let mtm = MainThreadMarker::new()?;
    let (commands, receiver) = smol::channel::unbounded();
    let target = CrabdashStatusTarget::new(mtm, commands);
    let bar = NSStatusBar::systemStatusBar();
    let item = bar.statusItemWithLength(-1.0); // NSVariableStatusItemLength
    let Some(button) = item.button(mtm) else {
        bar.removeStatusItem(&item);
        tracing::warn!("macOS could not create the Crabdash menu bar button");
        return None;
    };
    let title = NSString::from_str("Crabdash");
    // SF Symbols are public on macOS 11+, but older supported releases still
    // get a usable text item instead of an unavailable-selector exception.
    let symbols: bool = unsafe {
        msg_send![NSImage::class(), respondsToSelector: sel!(imageWithSystemSymbolName:accessibilityDescription:)]
    };
    let image = symbols
        .then(|| {
            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str("server.rack"),
                Some(&title),
            )
        })
        .flatten();
    if let Some(image) = image {
        image.setTemplate(true);
        button.setImage(Some(&image));
    } else {
        button.setTitle(&title);
    }
    button.setToolTip(Some(&NSString::from_str(
        "Crabdash — open dashboard or preferences",
    )));
    item.setAutosaveName(Some(&NSString::from_str("CrabdashStatusItem")));
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &title);
    menu.setAutoenablesItems(false);
    for (label, action) in [
        ("Show Crabdash", sel!(showCrabdash:)),
        ("Preferences…", sel!(openPreferences:)),
    ] {
        let entry = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(label),
                Some(action),
                &NSString::from_str(""),
            )
        };
        unsafe {
            entry.setTarget(Some(&target));
        }
        menu.addItem(&entry);
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    let quit = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str("Quit Crabdash"),
            Some(sel!(quitCrabdash:)),
            &NSString::from_str(""),
        )
    };
    unsafe {
        quit.setTarget(Some(&target));
    }
    menu.addItem(&quit);
    item.setMenu(Some(&menu));
    cx.set_global(TrayState {
        item: Some(item),
        _target: target,
    });
    cx.on_app_quit(|cx| {
        if let Some(item) = cx.global_mut::<TrayState>().item.take() {
            NSStatusBar::systemStatusBar().removeStatusItem(&item);
        }
        async {}
    })
    .detach();
    Some(receiver)
}

pub(crate) fn should_close(window: &mut Window, cx: &mut App) -> bool {
    if crate::features::preferences::current(cx).close_to_tray
        && cx
            .try_global::<TrayState>()
            .is_some_and(|state| state.item.as_ref().is_some_and(|item| item.isVisible()))
    {
        crate::desktop::window::hide_to_tray(window);
        false
    } else {
        true
    }
}
