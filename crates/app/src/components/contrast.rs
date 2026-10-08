//! Adaptive text contrast for semantic colours and native selection surfaces.
/// Choose opaque text with the greater contrast against normalized sRGB.
pub(crate) fn selection_text(components: [f64; 3]) -> u32 {
    let [red, green, blue] = components.map(|component| {
        if component <= 0.04045 {
            component / 12.92
        } else {
            ((component + 0.055) / 1.055).powf(2.4)
        }
    });
    let luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
    let black_contrast = (luminance + 0.05) / 0.05;
    let white_contrast = 1.05 / (luminance + 0.05);
    if black_contrast >= white_contrast {
        0x000000
    } else {
        0xFFFFFF
    }
}

/// Resolve foreground against a translucent tint over an opaque surface.
pub(crate) fn tinted_text(components: [f64; 3], alpha: f64, surface: u32) -> u32 {
    let alpha = alpha.clamp(0.0, 1.0);
    let base =
        [surface >> 16, surface >> 8, surface].map(|channel| f64::from(channel & 255) / 255.0);
    selection_text(std::array::from_fn(|index| {
        components[index] * alpha + base[index] * (1.0 - alpha)
    }))
}

#[cfg(test)]
mod tests {
    use super::{selection_text, tinted_text};

    #[test]
    fn translucent_tint_contrast_accounts_for_its_underlying_surface() {
        let green = [89.0 / 255.0, 198.0 / 255.0, 154.0 / 255.0];
        assert_eq!(tinted_text(green, 0.24, 0x181818), 0xFFFFFF);
        assert_eq!(tinted_text(green, 1.0, 0x181818), 0x000000);
        assert_eq!(tinted_text(green, 0.0, 0xFFFFFF), 0x000000);
    }

    #[test]
    fn bright_accents_use_dark_text_and_dark_accents_use_light_text() {
        for color in [[1.0; 3], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.5, 0.0]] {
            assert_eq!(selection_text(color), 0x000000);
        }
        for color in [[0.0; 3], [0.0, 0.0, 1.0], [0.4, 0.0, 0.5]] {
            assert_eq!(selection_text(color), 0xFFFFFF);
        }
    }

    #[test]
    fn selection_text_meets_normal_text_contrast_across_the_srgb_gamut() {
        // Independently calculate the displayed contrast, including colours
        // near the crossover where either foreground has its lowest contrast.
        for red in 0..=16 {
            for green in 0..=16 {
                for blue in 0..=16 {
                    let rgb = [red, green, blue].map(|channel| f64::from(channel) / 16.0);
                    let linear = rgb.map(|channel| {
                        if channel > 0.04045 {
                            ((channel + 0.055) / 1.055).powf(2.4)
                        } else {
                            channel / 12.92
                        }
                    });
                    let background = linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
                    let foreground: f64 = if selection_text(rgb) == 0 { 0.0 } else { 1.0 };
                    let contrast =
                        (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
                    assert!(contrast >= 4.5, "{rgb:?}: {contrast}");
                }
            }
        }
    }
}
