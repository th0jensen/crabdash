//! Saved appearance and the platform's live system appearance share one UI palette.
use gpui::{App, Global, Window, WindowAppearance, WindowId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub(crate) fn dark(self, system: WindowAppearance) -> bool {
        match self {
            Self::Light => false,
            Self::Dark => true,
            Self::System => matches!(
                system,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
        }
    }
}

#[derive(Default)]
struct Runtime {
    observed: HashSet<WindowId>,
    #[cfg(target_os = "macos")]
    native: Option<(WindowId, Appearance, bool)>,
}
impl Global for Runtime {}

pub(super) fn initialize(cx: &mut App) {
    let settings = crate::features::preferences::Preferences::load().unwrap_or_default();
    crate::components::style::set_dark(settings.appearance.dark(cx.window_appearance()));
    cx.set_global(Runtime::default());
}

/// GPUI updates this cached window value before notifying appearance observers.
/// Linux delivers portal updates while borrowing its platform state, so querying
/// App::window_appearance from that callback would borrow the same state again.
fn system_appearance(window: &Window, cx: &App) -> WindowAppearance {
    #[cfg(target_os = "macos")]
    {
        let _ = window;
        // AppKit's application appearance is independent of our explicit native
        // window override. Sampling the window would retain Light/Dark when the
        // preference changes back to System.
        cx.window_appearance()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = cx;
        window.appearance()
    }
}

/// Called before constructing any UI, including native controls and terminal snapshots.
pub(crate) fn synchronize(window: &mut Window, cx: &mut App) {
    let choice = crate::features::preferences::current(cx).appearance;
    let dark = choice.dark(system_appearance(window, cx));
    crate::components::style::set_dark(dark);
    let id = window.window_handle().window_id();
    if cx.default_global::<Runtime>().observed.insert(id) {
        window
            .observe_window_appearance(|window, cx| {
                let choice = crate::features::preferences::current(cx).appearance;
                if crate::components::style::set_dark(choice.dark(system_appearance(window, cx))) {
                    cx.refresh_windows();
                }
            })
            .detach();
    }
    #[cfg(target_os = "macos")]
    {
        let next = (id, choice, dark);
        let runtime = cx.default_global::<Runtime>();
        if runtime.native != Some(next) {
            runtime.native = Some(next);
            // AppKit appearance callbacks re-enter GPUI. Run outside its active App borrow.
            super::macos::schedule(window, choice, dark, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_follows_both_platform_variants_and_explicit_choices_do_not() {
        for system in [
            WindowAppearance::Light,
            WindowAppearance::VibrantLight,
            WindowAppearance::Dark,
            WindowAppearance::VibrantDark,
        ] {
            assert_eq!(
                Appearance::System.dark(system),
                matches!(
                    system,
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                )
            );
            assert!(!Appearance::Light.dark(system));
            assert!(Appearance::Dark.dark(system));
        }
    }
}
