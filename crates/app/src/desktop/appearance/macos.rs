//! AppKit owns sidebar and toolbar materials; dashboard content stays opaque.
use gpui::{Div, Hsla, Pixels, Window, div, prelude::*, px, rgb};
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSColor, NSColorSpace,
    NSWindow,
};
use std::cell::Cell;

/// Resolve the user's current accent in the same standard appearance as our
/// native windows. Dynamic colors need an RGB conversion before component access.
pub(crate) fn system_accent() -> Option<u32> {
    let _main_thread = MainThreadMarker::new()?;
    let appearance = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua })?;
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
    // The dashboard has a dark palette. Choose the matching standard AppKit
    // appearance for the whole window, keeping toolbar controls unstyled.
    if let Some(appearance) = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }) {
        window.setAppearance(Some(&appearance));
    }
    let color = crate::components::style::CONTENT;
    window.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from((color >> 16) & 0xff) / 255.0,
        f64::from((color >> 8) & 0xff) / 255.0,
        f64::from(color & 0xff) / 255.0,
        1.0,
    )));
}

pub(crate) fn root_background() -> Hsla {
    rgb(crate::components::style::CONTENT).into()
}
pub(crate) fn titlebar_background() -> Hsla {
    rgb(crate::components::style::CHROME).into()
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
        .bg(rgb(crate::components::style::BORDER))
}

/// The main content owns this divider, so it stops below the native toolbar.
pub(crate) fn sidebar_separator() -> Div {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .w(px(1.0))
        .bg(rgb(crate::components::style::BORDER))
}
