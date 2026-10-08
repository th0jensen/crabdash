//! Desktop accent state and UI invalidation, shared by all dashboard domains.
use gpui::App;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use gpui::Global;

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[derive(Default)]
struct Accent {
    color: Option<u32>,
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
impl Global for Accent {}

pub(crate) fn initialize(cx: &mut App) {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        cx.set_global(Accent::default());
        #[cfg(target_os = "linux")]
        let updates = super::linux_accent::start(cx);
        #[cfg(target_os = "windows")]
        let updates = super::windows_accent::start(cx);
        if let Some(updates) = updates {
            cx.spawn(async move |cx| {
                while let Ok(color) = updates.recv().await {
                    if cx
                        .update(|cx| {
                            let accent = cx.global_mut::<Accent>();
                            if accent.color != color {
                                accent.color = color;
                                cx.refresh_windows();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
    }
    #[cfg(target_os = "macos")]
    let _ = cx;
}

pub(crate) fn system_accent(enabled: bool, cx: &App) -> Option<u32> {
    if !enabled {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = cx;
        super::macos::system_accent()
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        cx.try_global::<Accent>().and_then(|accent| accent.color)
    }
}

/// The portal uses normalized sRGB, with out-of-range values meaning unset.
#[cfg(any(target_os = "linux", test))]
pub(super) fn rgb_from_components(components: [f64; 3]) -> Option<u32> {
    if !components
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        return None;
    }
    let [red, green, blue] = components.map(|value| (value * 255.0).round() as u32);
    Some((red << 16) | (green << 8) | blue)
}

#[cfg(test)]
mod tests {
    use super::rgb_from_components;
    #[test]
    fn normalized_srgb_preserves_black_and_white_and_rounds_components() {
        assert_eq!(rgb_from_components([0.0; 3]), Some(0));
        assert_eq!(rgb_from_components([1.0; 3]), Some(0xFFFFFF));
        assert_eq!(rgb_from_components([1.0, 0.5, 0.0]), Some(0xFF8000));
    }
    #[test]
    fn unset_and_malformed_portal_colors_have_no_accent() {
        for invalid in [-1.0, 1.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for component in 0..3 {
                let mut color = [0.5; 3];
                color[component] = invalid;
                assert_eq!(rgb_from_components(color), None);
            }
        }
    }
}
