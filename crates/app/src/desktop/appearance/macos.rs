//! Real AppKit material only behind the top navigation chrome. Data remains opaque.
use gpui::{Div, Hsla, Pixels, Window, px, rgb, rgba};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSAutoresizingMaskOptions,
    NSUserInterfaceItemIdentification, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const MATERIAL_ID: &str = "CrabdashNavigationMaterial";
pub(crate) fn root_background() -> Hsla {
    if reduced_transparency() {
        rgb(crate::components::style::CONTENT).into()
    } else {
        rgba(0x00000000).into()
    }
}
pub(crate) fn titlebar_background() -> Hsla {
    if reduced_transparency() {
        rgb(crate::components::style::CHROME).into()
    } else {
        rgba(0x20202030).into()
    }
}
pub(crate) fn reduced_transparency() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
}
fn native_view(window: &Window) -> Option<&NSView> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: GPUI exposes its live AppKit NSView; the borrow cannot outlive the
    // GPUI Window and all callers execute on the main UI thread.
    Some(unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() })
}
fn bounds(parent: &NSView, window: &Window) -> NSRect {
    let bounds = parent.bounds();
    let height =
        f64::from(f32::from(window.rem_size()) * crate::components::style::TITLE_BAR / 16.0);
    NSRect::new(
        NSPoint::new(
            0.0,
            if parent.isFlipped() {
                0.0
            } else {
                bounds.size.height - height
            },
        ),
        NSSize::new(bounds.size.width, height),
    )
}
pub(crate) fn frame(root: Div, window: &Window) -> Div {
    // Interface font scale can change after launch; synchronize the native
    // material's bounds with the GPUI title bar on each render as well as resize.
    if let Some(view) = native_view(window) {
        if let Some(parent) = unsafe { view.superview() } {
            for child in parent.subviews() {
                if child
                    .identifier()
                    .is_some_and(|id| id.to_string() == MATERIAL_ID)
                {
                    child.setFrame(bounds(&parent, window));
                    child.setHidden(reduced_transparency());
                    break;
                }
            }
        }
    }
    root
}
pub(super) fn prepare(window: &Window) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(view) = native_view(window) else {
        return;
    };
    let Some(parent) = (unsafe { view.superview() }) else {
        return;
    };
    let frame = bounds(&parent, window);
    // Glass belongs to the real NSButtons above the GPU view. A sibling glass
    // view with an empty contentView cannot correctly host GPUI controls.
    let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), frame);
    effect.setMaterial(NSVisualEffectMaterial::HeaderView);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::FollowsWindowActiveState);
    effect.setHidden(reduced_transparency());
    // Crabdash currently uses a dark palette; a light native material would
    // wash out its fixed foreground colors. AppKit still manages accessibility
    // contrast, reduced transparency and active/inactive material behavior.
    if let Some(appearance) = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }) {
        effect.setAppearance(Some(&appearance));
    }
    effect.setIdentifier(Some(&NSString::from_str(MATERIAL_ID)));
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable
            | if parent.isFlipped() {
                NSAutoresizingMaskOptions::ViewMaxYMargin
            } else {
                NSAutoresizingMaskOptions::ViewMinYMargin
            },
    );
    // AppKit owns the backing after insertion; placing it below the GPU view
    // preserves GPUI hit testing, keyboard focus, menus and traffic-light controls.
    parent.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, Some(view));
}

/// AppKit owns the outer frame; GPUI adds no border inside the client area.
pub(crate) fn frame_inset(_: &Window) -> Pixels {
    px(0.0)
}
