use std::borrow::Cow;

use gpui::App;

pub mod app;
pub mod components;
pub mod content;
pub use app::Crabdash;

pub fn register_fonts(cx: &mut App) {
    cx.text_system()
        .add_fonts(vec![Cow::Borrowed(lucide_icons::LUCIDE_FONT_BYTES)])
        .expect("failed to load lucide icon font");
}
