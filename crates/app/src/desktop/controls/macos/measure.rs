//! Bounded stock intrinsic-size cache; measurement never resizes visible leaves.
use super::{Spec, native::Content};
use gpui::{App, Global, Window};
use objc2::MainThreadMarker;
use objc2_app_kit::NSImage;
use objc2_foundation::{NSSize, NSString};
use std::collections::VecDeque;

#[derive(PartialEq)]
struct Key {
    kind: u8,
    title: String,
    icon: Option<&'static str>,
    choices: Option<Vec<String>>,
    toggle: bool,
    toolbar: bool,
}
#[derive(Default)]
struct Sizes(VecDeque<(Key, NSSize)>);
impl Global for Sizes {}
#[derive(Default)]
struct Symbols(VecDeque<((&'static str, String), objc2::rc::Retained<NSImage>)>);
impl Global for Symbols {}

pub(super) fn symbol_image(
    icon: lucide_icons::Icon,
    accessibility: &str,
    selected: Option<bool>,
    window: &Window,
    cx: &mut App,
) -> Option<objc2::rc::Retained<NSImage>> {
    let _ = window;
    let key = (
        super::widgets::button_symbol(icon, selected),
        accessibility.to_string(),
    );
    if let Some((_, image)) = cx
        .default_global::<Symbols>()
        .0
        .iter()
        .find(|(old, _)| old == &key)
    {
        return Some(image.clone());
    }
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(super::widgets::button_symbol(icon, selected)),
        Some(&NSString::from_str(accessibility)),
    )?;
    // Preserve AppKit's default symbol configuration and control metrics.
    let symbols = &mut cx.default_global::<Symbols>().0;
    if symbols.len() >= 128 {
        symbols.pop_front();
    }
    symbols.push_back((key, image.clone()));
    Some(image)
}

pub(super) fn size(spec: &Spec, window: &Window, cx: &mut App) -> Option<NSSize> {
    let (kind, title, icon) = match spec {
        Spec::Button {
            label,
            icon,
            selected,
            ..
        } => (
            0,
            label.to_string(),
            icon.map(|icon| super::widgets::button_symbol(icon, *selected)),
        ),
        Spec::Search { .. } => (1, String::new(), None),
        Spec::Popup { help, .. } => (4, help.to_string(), None),
        Spec::Switch { .. } => (2, String::new(), None),
        Spec::Status { label, .. } => (3, label.to_string(), None),
    };
    let key = Key {
        kind,
        title,
        icon,
        choices: match spec {
            Spec::Popup { values, .. } => Some(values.clone()),
            _ => None,
        },
        toggle: matches!(
            spec,
            Spec::Button {
                selected: Some(_),
                ..
            }
        ),
        toolbar: matches!(
            spec,
            Spec::Button {
                role: super::ButtonRole::Toolbar,
                ..
            }
        ),
    };
    if let Some((_, size)) = cx
        .default_global::<Sizes>()
        .0
        .iter()
        .find(|(old, _)| old == &key)
    {
        return Some(*size);
    }
    // Unattached controls can measure stock cell metrics without creating any
    // retained leaf, target, field editor, or hierarchy while GPUI requests layout.
    let content = Content::new(spec, MainThreadMarker::new()?);
    if let (
        Content::Button(button),
        Spec::Button {
            icon: Some(icon),
            accessibility,
            selected,
            ..
        },
    ) = (&content, spec)
    {
        button.setImage(symbol_image(*icon, accessibility, *selected, window, cx).as_deref());
    }
    content.control().sizeToFit();
    let mut size = content.view().frame().size;
    if kind == 3 {
        size.width += 12.0;
        size.height += 8.0;
    }
    if kind == 2 {
        size.width += 6.0;
        size.height += 6.0;
    }
    let sizes = &mut cx.default_global::<Sizes>().0;
    if sizes.len() >= 64 {
        sizes.pop_front();
    }
    sizes.push_back((key, size));
    Some(size)
}
