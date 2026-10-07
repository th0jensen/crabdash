//! AppKit buttons occupy the exact layout rectangles reserved by GPUI.
use super::Control;
use crate::app::Crabdash;
use gpui::*;
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, runtime::AnyClass, sel,
};
use objc2_app_kit::{
    NSAccessibility, NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua,
    NSBezelStyle, NSButton, NSButtonType, NSCellImagePosition, NSControlSize, NSFontWeightRegular,
    NSImage, NSImageScaling, NSImageSymbolConfiguration, NSView, NSWorkspace,
    NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use smol::channel::Sender;
use std::cell::Cell;

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the weak button target
    // is retained by NativeButton for the entire native view lifetime.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Sender<()>]
    struct ButtonTarget;
    unsafe impl NSObjectProtocol for ButtonTarget {}
    impl ButtonTarget {
        #[unsafe(method(invokeCrabdashControl:))]
        fn invoke(&self, _: Option<&NSObject>) { let _ = self.ivars().try_send(()); }
    }
);
impl ButtonTarget {
    fn new(mtm: MainThreadMarker, commands: Sender<()>) -> Retained<Self> {
        let allocated = Self::alloc(mtm).set_ivars(commands);
        // SAFETY: NSObject initializes the allocated action target.
        unsafe { msg_send![super(allocated), init] }
    }
}

define_class!(
    // SAFETY: Notification delivery only queues work; GPUI is never reentered.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Sender<()>]
    struct DisplayTarget;
    unsafe impl NSObjectProtocol for DisplayTarget {}
    impl DisplayTarget {
        #[unsafe(method(crabdashDisplayOptionsChanged:))]
        fn changed(&self, _: Option<&NSNotification>) { let _ = self.ivars().try_send(()); }
    }
);
struct DisplayObserver {
    center: Retained<NSNotificationCenter>,
    target: Retained<DisplayTarget>,
}
impl Global for DisplayObserver {}
impl Drop for DisplayObserver {
    fn drop(&mut self) {
        // SAFETY: This object registered its retained target on this center.
        unsafe {
            self.center.removeObserver(&self.target);
        }
    }
}
fn observe_display_options(cx: &mut App) {
    if cx.try_global::<DisplayObserver>().is_some() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let (commands, receiver) = smol::channel::bounded(1);
    let allocated = DisplayTarget::alloc(mtm).set_ivars(commands);
    // SAFETY: NSObject initializes the allocated notification target.
    let target: Retained<DisplayTarget> = unsafe { msg_send![super(allocated), init] };
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: The selector accepts NSNotification and the target is retained.
    unsafe {
        center.addObserver_selector_name_object(
            &target,
            sel!(crabdashDisplayOptionsChanged:),
            Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
            None,
        );
    }
    cx.set_global(DisplayObserver { center, target });
    cx.spawn(async move |cx| {
        while receiver.recv().await.is_ok() {
            if cx.update(|cx| cx.refresh_windows()).is_err() {
                break;
            }
        }
    })
    .detach();
}

fn native_view(window: &Window) -> Option<&NSView> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: GPUI owns this live NSView; callers run on its main UI thread.
    Some(unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() })
}

fn restore_renderer_focus(window: &Window) {
    if let Some(view) = native_view(window) {
        if let Some(native) = view.window() {
            if !native.makeFirstResponder(Some(view)) {
                tracing::warn!("AppKit could not restore Crabdash keyboard input");
            }
        }
    }
}

struct NativeButton {
    button: Retained<NSButton>,
    _target: Retained<ButtonTarget>,
    symbol_point_size: Cell<Option<f64>>,
}
impl Drop for NativeButton {
    fn drop(&mut self) {
        // SAFETY: Clear AppKit's weak callback before dropping its target.
        unsafe {
            self.button.setTarget(None);
            self.button.setAction(None);
        }
        self.button.removeFromSuperview();
    }
}
impl NativeButton {
    fn set_visible(&self, visible: bool, window: &Window) {
        if !visible {
            if let Some(native) = self.button.window() {
                if native.firstResponder().is_some_and(|responder| {
                    Retained::as_ptr(&responder).cast::<()>()
                        == Retained::as_ptr(&self.button).cast::<()>()
                }) {
                    restore_renderer_focus(window);
                }
            }
        }
        self.button.setHidden(!visible);
    }
    fn new(control: Control, window: &Window, cx: &mut App) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let view = native_view(window)?;
        // Older runtimes lack SF Symbols. Preserve the usable GPUI fallback.
        // SAFETY: Ask the class object about its public class selector, rather
        // than checking instance methods with AnyClass::responds_to.
        let symbols: bool = unsafe {
            msg_send![NSImage::class(), respondsToSelector: sel!(imageWithSystemSymbolName:accessibilityDescription:)]
        };
        if !symbols {
            return None;
        }
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(match control {
                Control::Refresh => "arrow.clockwise",
                Control::Terminal => "terminal",
            }),
            Some(&NSString::from_str(control.tooltip())),
        )?;
        image.setTemplate(true);
        let button = NSButton::initWithFrame(NSButton::alloc(mtm), NSRect::ZERO);
        button.setHidden(true);
        button.setTitle(&NSString::from_str(""));
        button.setImage(Some(&image));
        button.setImagePosition(NSCellImagePosition::ImageOnly);
        button.setImageScaling(NSImageScaling::ScaleProportionallyDown);
        button.setControlSize(NSControlSize::Small);
        button.setButtonType(match control {
            Control::Refresh => NSButtonType::MomentaryPushIn,
            Control::Terminal => NSButtonType::PushOnPushOff,
        });
        button.setToolTip(Some(&NSString::from_str(control.tooltip())));
        button.setAccessibilityLabel(Some(&NSString::from_str(match control {
            Control::Refresh => "Refresh",
            Control::Terminal => "Toggle terminal",
        })));
        button.setAccessibilityHelp(Some(&NSString::from_str(control.tooltip())));
        if let Some(appearance) = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua })
        {
            button.setAppearance(Some(&appearance));
        }
        let (commands, receiver) = smol::channel::unbounded();
        let target = ButtonTarget::new(mtm, commands);
        // SAFETY: The target outlives the weak NSButton target and implements this selector.
        unsafe {
            button.setTarget(Some(&target));
            button.setAction(Some(sel!(invokeCrabdashControl:)));
        }
        view.addSubview(&button);
        let handle = window.window_handle();
        cx.spawn(async move |cx| {
            while receiver.recv().await.is_ok() {
                let result = cx.update(|cx| {
                    handle.update(cx, |_, window, cx| {
                        restore_renderer_focus(window);
                        window.dispatch_action(control.action(), cx)
                    })
                });
                if !matches!(result, Ok(Ok(()))) {
                    break;
                }
            }
        })
        .detach();
        Some(Self {
            button,
            _target: target,
            symbol_point_size: Cell::new(None),
        })
    }
    fn synchronize(&self, bounds: Bounds<Pixels>, selected: bool, window: &Window) -> bool {
        let Some(view) = native_view(window) else {
            self.button.setHidden(true);
            return false;
        };
        let symbol_point_size = f64::from(crate::components::style::ICON)
            * f64::from(f32::from(window.rem_size()))
            / 16.0;
        if self.symbol_point_size.get() != Some(symbol_point_size)
            && self
                .button
                .respondsToSelector(sel!(setSymbolConfiguration:))
        {
            // Both symbol configuration and the guarded symbol-image factory
            // are public macOS 11 APIs. Older runtimes use the GPUI fallback.
            let configuration = NSImageSymbolConfiguration::configurationWithPointSize_weight(
                symbol_point_size,
                // SAFETY: AppKit exports this immutable regular font weight.
                unsafe { NSFontWeightRegular },
            );
            self.button.setSymbolConfiguration(Some(&configuration));
            self.symbol_point_size.set(Some(symbol_point_size));
        }
        let glass = AnyClass::get(c"NSGlassEffectView").is_some()
            && !crate::desktop::appearance::reduced_transparency();
        self.button.setBezelStyle(if glass {
            NSBezelStyle::Glass
        } else {
            NSBezelStyle::Push
        });
        // The public macOS 26 selector is absent on older supported releases.
        if self
            .button
            .respondsToSelector(sel!(setPrefersCompactControlSizeMetrics:))
        {
            self.button.setPrefersCompactControlSizeMetrics(true);
        }
        self.button.setState(if selected { 1 } else { 0 });
        let y = if view.isFlipped() {
            f64::from(f32::from(bounds.origin.y))
        } else {
            view.bounds().size.height - f64::from(f32::from(bounds.bottom()))
        };
        self.button.setFrame(NSRect::new(
            NSPoint::new(f64::from(f32::from(bounds.origin.x)), y),
            NSSize::new(
                f64::from(f32::from(bounds.size.width)),
                f64::from(f32::from(bounds.size.height)),
            ),
        ));
        true
    }
}

#[derive(Default)]
struct NativeState {
    button: Option<NativeButton>,
}
struct NativeControl {
    control: Control,
    selected: bool,
    blocked: bool,
    fallback: Option<AnyElement>,
}
impl IntoElement for NativeControl {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for NativeControl {
    type RequestLayoutState = AnyElement;
    type PrepaintState = bool;
    fn id(&self) -> Option<ElementId> {
        Some(
            match self.control {
                Control::Refresh => "native-refresh",
                Control::Terminal => "native-terminal",
            }
            .into(),
        )
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        let mut fallback = self
            .fallback
            .take()
            .unwrap_or_else(|| div().into_any_element());
        (fallback.request_layout(window, cx), fallback)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        fallback: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        fallback.prepaint(window, cx);
        observe_display_options(cx);
        let Some(id) = id else {
            return false;
        };
        let unclipped = window.content_mask().bounds.intersect(&bounds) == bounds;
        window.with_element_state::<NativeState, _>(id, |state, window| {
            let mut state = state.unwrap_or_default();
            if state.button.is_none() {
                state.button = NativeButton::new(self.control, window, cx);
            }
            let native = state.button.as_ref().is_some_and(|button| {
                if self.blocked || !unclipped {
                    // Keep native focus out of an obscured or clipped control.
                    button.set_visible(false, window);
                }
                button.synchronize(bounds, self.selected, window)
            });
            (native && unclipped && !self.blocked, state)
        })
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        fallback: &mut AnyElement,
        native: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let visible = *native && !cx.has_active_drag() && !window.has_native_overlay_occluder();
        if let Some(id) = id {
            window.with_element_state::<NativeState, _>(id, |state, window| {
                let state = state.unwrap_or_default();
                if let Some(button) = &state.button {
                    button.set_visible(visible, window);
                }
                ((), state)
            });
        }
        if !visible {
            fallback.paint(window, cx);
        }
    }
}

pub(super) fn render(
    control: Control,
    app: &Crabdash,
    _: &mut Window,
    cx: &mut Context<Crabdash>,
) -> AnyElement {
    NativeControl {
        control,
        selected: control == Control::Terminal && app.quake_terminal_open,
        blocked: app.preferences_open
            || app.add_machine_modal_open
            || app.docker_run_modal_open
            || app.docker_removal.is_some()
            || app.open_menu.is_some()
            || app.workspaces.open,
        fallback: Some(super::fallback(control, app, cx).into_any_element()),
    }
    .into_any_element()
}
