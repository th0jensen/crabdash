mod element;
mod group;
mod leaf;
mod measure;
mod native;
mod widgets;
pub(crate) use element::Control as SurfaceControl;
pub(crate) use group::Actions;
pub(crate) use widgets::ButtonRole;
use widgets::Event;
pub(super) use widgets::Spec;

// GPUI fallback used only if the native AppKit shell cannot be installed.
use crate::app::Crabdash;
use gpui::{prelude::*, *};

pub(super) fn action_group_width(
    shared: Pixels,
    slots: &[&[lucide_icons::Icon]],
    liquid_glass: bool,
    toolbar: bool,
    window: &Window,
    cx: &mut App,
) -> Pixels {
    // Called while Crabdash is rendering: use its supplied preference instead
    // of borrowing the same entity again through Window::root.
    if !liquid_glass || objc2::runtime::AnyClass::get(c"NSGlassEffectView").is_none() {
        return shared;
    }
    let minimum = px(crate::components::style::CONTROL * f32::from(window.rem_size()) / 16.0);
    let mut width = px(4.0) * slots.len().saturating_sub(1) as f32;
    for (index, icons) in slots.iter().enumerate() {
        let mut slot = minimum;
        for icon in *icons {
            let spec = Spec::Button {
                label: SharedString::default(),
                accessibility: SharedString::default(),
                icon: Some(*icon),
                selected: (!toolbar && index == 0).then_some(false),
                role: if toolbar {
                    ButtonRole::Toolbar
                } else {
                    ButtonRole::Normal
                },
            };
            if let Some(size) = measure::size(&spec, window, cx) {
                slot = slot.max(px(size.width as f32));
            }
        }
        width += slot;
    }
    shared.max(width)
}

pub(super) fn render(
    control: super::Control,
    app: &Crabdash,
    _: &mut Window,
    cx: &mut Context<Crabdash>,
) -> AnyElement {
    super::fallback(control, app, cx).into_any_element()
}

pub(super) fn active(liquid_glass: bool) -> bool {
    // Render callers already own Crabdash; never reborrow its root entity here.
    liquid_glass && objc2::runtime::AnyClass::get(c"NSGlassEffectView").is_some()
}
