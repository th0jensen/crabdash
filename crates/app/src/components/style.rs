//! Shared sizing for dashboard chrome and controls.
use std::sync::atomic::{AtomicBool, Ordering};

static DARK: AtomicBool = AtomicBool::new(true);

pub(crate) fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}
pub(crate) fn set_dark(dark: bool) -> bool {
    DARK.swap(dark, Ordering::Relaxed) != dark
}

/// UI palette lookup. Dynamic accents and terminal ANSI colours bypass this mapping.
pub(crate) fn color(value: u32) -> u32 {
    color_for(value, is_dark())
}
pub(crate) fn rgb(value: u32) -> gpui::Rgba {
    gpui::rgb(color(value))
}
pub(crate) fn rgba(value: u32) -> gpui::Rgba {
    gpui::rgba((color(value >> 8) << 8) | (value & 0xff))
}

pub(crate) fn color_for(value: u32, dark: bool) -> u32 {
    if dark {
        return value;
    }
    match value {
        CONTENT => 0xfafafa,
        CHROME => 0xf0f0f0,
        SURFACE_HOVER => 0xe8e8e8,
        BORDER => 0xd5d5d5,
        CONTROL_SELECTED_BORDER => 0xa0a0a0,
        FOCUS_BORDER => 0x686868,
        TAB_INDICATOR => 0x555555,
        TAB_CLOSE => 0x767676,
        TEXT_PRIMARY => 0x242424,
        TEXT_SELECTED => 0x121212,
        TEXT_MUTED => 0x656565,
        SUCCESS => 0x16764b,
        WARNING => 0x906200,
        DANGER => 0xb42332,
        0xff9f99 | 0xff7068 | 0xff453a | 0xf2b6b2 => 0xb42332,
        0x47232b | 0x4a252c => 0xfae7e9,
        0x8ab4ff => 0x2455a4,
        0x252f3d | 0x2b3849 => 0xe0e8f2,
        0x3c506a | 0x304158 => 0xb9c9dc,
        0xa9ceff => 0x2455a4,
        0x383b3d => 0xb8c0c6,
        // Existing UI-only neutral shades retain their roles in the light palette.
        0x111111 | 0x101010 | 0x151515 | 0x161616 | 0x191919 | 0x1a1a1a => 0xfafafa,
        0x1e1e1e | 0x222222 | 0x242424 | 0x252525 | 0x262626 => 0xf0f0f0,
        0x2a2a2a | 0x2c2c2c | 0x323232 | 0x333333 | 0x363636 | 0x383838 | 0x404040 | 0x444444 => {
            0xd5d5d5
        }
        0x666666 | 0x777777 | 0x888888 | 0x8a8a8a | 0x8c8c8c | 0x909090 | 0x929292 | 0x999999 => {
            0x656565
        }
        0xaaaaaa | 0xb0b0b0 | 0xbbbbbb | 0xc0c0c0 | 0xcccccc | 0xd0d0d0 | 0xdddddd | 0xe5e5e5
        | 0xe6e6e6 | 0xeeeeee | 0xffffff => 0x242424,
        _ => {
            let channels = [(value >> 16) & 255, (value >> 8) & 255, value & 255];
            let min = *channels.iter().min().unwrap_or(&0);
            let max = *channels.iter().max().unwrap_or(&0);
            if max - min <= 8 {
                match (channels[0] + channels[1] + channels[2]) / 3 {
                    0 => value,
                    1..=40 => 0xf6f6f6,
                    41..=80 => 0xd5d5d5,
                    81..=185 => 0x656565,
                    _ => 0x242424,
                }
            } else {
                value
            }
        }
    }
}
pub(crate) const TEXT: f32 = 14.0;
pub(crate) const META: f32 = 12.0;
pub(crate) const ICON: f32 = 14.0;
pub(crate) const CONTROL: f32 = 28.0;
pub(crate) const CHROME_CONTROL: f32 = 24.0;
pub(crate) const BAR: f32 = 32.0;
pub(crate) const TITLE_BAR: f32 = 34.0;
pub(crate) const RADIUS: f32 = 4.0;

// Neutral chrome keeps selection and dashboard surfaces visually connected.
pub(crate) const CONTENT: u32 = 0x181818;
pub(crate) const CHROME: u32 = 0x202020;
pub(crate) const TEXT_PRIMARY: u32 = 0xE0E0E0;
pub(crate) const TEXT_SELECTED: u32 = 0xF2F2F2;
pub(crate) const TEXT_MUTED: u32 = 0x969696;
pub(crate) const SURFACE: u32 = CONTENT;
pub(crate) const SURFACE_HOVER: u32 = 0x282828;
pub(crate) const BORDER: u32 = 0x303030;
pub(crate) const CONTROL_SELECTED_BG: u32 = 0x303030;
pub(crate) const CONTROL_SELECTED_BORDER: u32 = 0x555555;
pub(crate) const FOCUS_BORDER: u32 = 0x707070;
pub(crate) const TAB_INDICATOR: u32 = 0xA0A0A0;
pub(crate) const TAB_CLOSE: u32 = 0x878787;
pub(crate) const SUCCESS: u32 = 0x59C69A;
pub(crate) const WARNING: u32 = 0xE4BB72;
pub(crate) const DANGER: u32 = 0xF08B86;
pub(crate) const STATE_TINT_ALPHA: f32 = 0.24;
pub(crate) const CARD_RADIUS: f32 = 4.0;
pub(crate) const LIGHT_TERMINAL_ANSI: [u32; 16] = [
    0x242424, 0xa51c30, 0x17663b, 0x806000, 0x2455a4, 0x7f3294, 0x006b78, 0x6d6d6d, 0x555555,
    0xc72c41, 0x24814b, 0x967000, 0x356dcc, 0x9848ae, 0x00818f, 0x444444,
];

#[cfg(test)]
mod theme_tests {
    use super::*;
    #[test]
    fn palette_keeps_surfaces_joined_and_text_contrasting() {
        for dark in [false, true] {
            assert_eq!(color_for(SURFACE, dark), color_for(CONTENT, dark));
            assert_ne!(color_for(TEXT_PRIMARY, dark), color_for(CONTENT, dark));
            assert_ne!(color_for(BORDER, dark), color_for(CONTENT, dark));
        }
        assert_eq!(color_for(CONTENT, true), CONTENT);
        assert_eq!(color_for(CONTENT, false), 0xfafafa);
        assert_eq!(color_for(TEXT_PRIMARY, false), 0x242424);
        assert_eq!(color_for(0x12abef, false), 0x12abef);
    }
}
