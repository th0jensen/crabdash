//! The terminal's own colors, which Rex reports from its theme. Pills get a
//! surface between background and foreground, as ramp mixes its surface.
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

use ratatui::style::{Color, Modifier, Style};

/// How far the pill surface moves from the background toward the foreground.
const SURFACE: f32 = 0.14;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub pill: Style,
}

impl Default for Palette {
    /// Reverse video stands out in any theme when the terminal reports nothing.
    fn default() -> Self {
        Self {
            pill: Style::new().add_modifier(Modifier::REVERSED),
        }
    }
}

impl Palette {
    /// Asks the terminal for its colors. Call in raw mode, before reading events.
    pub fn query() -> Self {
        platform::query()
            .and_then(|reply| Self::from_reply(&reply))
            .unwrap_or_default()
    }

    fn from_reply(reply: &str) -> Option<Self> {
        let foreground = color_after(reply, "]10;")?;
        let background = color_after(reply, "]11;")?;
        let mix =
            |b: u8, f: u8| (f32::from(b) + (f32::from(f) - f32::from(b)) * SURFACE).round() as u8;
        let surface = Color::Rgb(
            mix(background[0], foreground[0]),
            mix(background[1], foreground[1]),
            mix(background[2], foreground[2]),
        );
        Some(Self {
            pill: Style::new().bg(surface),
        })
    }
}

/// Parses `rgb:RRRR/GGGG/BBBB` (one to four hex digits each) after `marker`.
fn color_after(reply: &str, marker: &str) -> Option<[u8; 3]> {
    let rest = &reply[reply.find(marker)? + marker.len()..];
    let rest = rest.strip_prefix("rgb:")?;
    let mut channels = [0u8; 3];
    let mut parts = rest.split('/');
    for channel in &mut channels {
        let digits: String = parts
            .next()?
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect();
        let value = u32::from_str_radix(&digits, 16).ok()?;
        let max = (1u32 << (4 * digits.len() as u32)) - 1;
        *channel = (value * 255 / max) as u8;
    }
    Some(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_surface_mixes_reported_background_toward_foreground() {
        let reply = "\x1b]10;rgb:ffff/ffff/ffff\x1b\\\x1b]11;rgb:00/20/40\x07\x1b[?62;22c";
        let palette = Palette::from_reply(reply);
        // 0x00 + 255 * 0.14 = 35.7, 0x20 + 223 * 0.14 = 63.2, 0x40 + 191 * 0.14 = 90.7.
        assert_eq!(
            palette.map(|p| p.pill),
            Some(Style::new().bg(Color::Rgb(36, 63, 91)))
        );
        assert_eq!(Palette::from_reply("\x1b[?62;22c"), None);
        assert_eq!(color_after("]11;rgb:f/8/0", "]11;"), Some([255, 136, 0]));
    }
}
