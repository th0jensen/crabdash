//! AppKit owns sidebar and toolbar materials; dashboard content stays opaque.
use gpui::{App, Div, Hsla, Pixels, Window, div, prelude::*, px};
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSColor, NSColorSpace, NSView, NSWindow,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::Cell;

/// Resolve the user's current accent in the same standard appearance as our
/// native windows. Dynamic colors need an RGB conversion before component access.
pub(crate) fn system_accent() -> Option<u32> {
    let _main_thread = MainThreadMarker::new()?;
    let name = if crate::components::style::is_dark() {
        unsafe { NSAppearanceNameDarkAqua }
    } else {
        unsafe { NSAppearanceNameAqua }
    };
    let appearance = NSAppearance::appearanceNamed(name)?;
    let accent = Cell::new(None);
    let resolve = block2::StackBlock::new(|| {
        let Some(color) =
            NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
        else {
            return;
        };
        // The successful sRGB conversion makes these component selectors valid,
        // including when the original dynamic color has another color-space model.
        let components = [
            color.redComponent(),
            color.greenComponent(),
            color.blueComponent(),
        ];
        if components.iter().all(|component| component.is_finite()) {
            let [red, green, blue] =
                components.map(|component| (component.clamp(0.0, 1.0) * 255.0).round() as u32);
            accent.set(Some((red << 16) | (green << 8) | blue));
        }
    });
    appearance.performAsCurrentDrawingAppearance(&resolve);
    accent.get()
}

pub(in crate::desktop) fn prepare(window: &NSWindow) {
    // Leave appearance inherited until the saved choice is synchronized.
    let color = crate::components::style::color(crate::components::style::CONTENT);
    window.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from((color >> 16) & 0xff) / 255.0,
        f64::from((color >> 8) & 0xff) / 255.0,
        f64::from(color & 0xff) / 255.0,
        1.0,
    )));
}

pub(super) fn schedule(window: &Window, choice: super::Appearance, dark: bool, cx: &App) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    // SAFETY: GPUI owns the live view; its window is retained for a UI-thread task.
    let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
    let Some(native) = view.window() else {
        return;
    };
    cx.foreground_executor()
        .spawn(async move {
            let appearance = match choice {
                super::Appearance::System => None,
                super::Appearance::Light => {
                    NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua })
                }
                super::Appearance::Dark => {
                    NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua })
                }
            };
            native.setAppearance(appearance.as_deref());
            let color =
                crate::components::style::color_for(crate::components::style::CONTENT, dark);
            native.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
                f64::from((color >> 16) & 255) / 255.0,
                f64::from((color >> 8) & 255) / 255.0,
                f64::from(color & 255) / 255.0,
                1.0,
            )));
        })
        .detach();
}

pub(crate) fn root_background() -> Hsla {
    crate::components::style::rgb(crate::components::style::CONTENT).into()
}
pub(crate) fn titlebar_background() -> Hsla {
    crate::components::style::rgb(crate::components::style::CHROME).into()
}
pub(crate) fn frame(root: Div, _: &Window) -> Div {
    root
}
pub(crate) fn frame_inset(_: &Window) -> Pixels {
    px(0.0)
}

/// Draw the dashboard's shared divider color directly below the native toolbar.
/// The decoration has no hitbox and does not change the content viewport.
pub(crate) fn toolbar_separator() -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(1.0))
        .bg(crate::components::style::rgb(
            crate::components::style::BORDER,
        ))
}

/// The main content owns this divider, so it stops below the native toolbar.
pub(crate) fn sidebar_separator() -> Div {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .w(px(1.0))
        .bg(crate::components::style::rgb(
            crate::components::style::BORDER,
        ))
}
