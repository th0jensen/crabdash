//! Bundled font registration; user-selected and native default families stay intact.
use gpui::App;
use std::borrow::Cow;

pub const JETBRAINS_MONO_NERD_REGULAR: &[u8] =
    include_bytes!("../assets/JetBrainsMonoNerdFont-Regular.ttf");
pub const JETBRAINS_MONO_NERD_BOLD: &[u8] =
    include_bytes!("../assets/JetBrainsMonoNerdFont-Bold.ttf");
pub const JETBRAINS_MONO_NERD_ITALIC: &[u8] =
    include_bytes!("../assets/JetBrainsMonoNerdFont-Italic.ttf");
pub const JETBRAINS_MONO_NERD_BOLD_ITALIC: &[u8] =
    include_bytes!("../assets/JetBrainsMonoNerdFont-BoldItalic.ttf");

const IBM_PLEX_SANS_REGULAR: &[u8] =
    include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
const IBM_PLEX_SANS_SEMIBOLD: &[u8] =
    include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-SemiBold.ttf");

pub fn register_fonts(cx: &mut App) {
    // Register independently so a failed optional UI face cannot discard the
    // icons or terminal faces. GPUI falls back when an unavailable font is used.
    for (face, bytes) in [
        ("Lucide", lucide_icons::LUCIDE_FONT_BYTES),
        ("JetBrains Mono Regular", JETBRAINS_MONO_NERD_REGULAR),
        ("JetBrains Mono Bold", JETBRAINS_MONO_NERD_BOLD),
        ("JetBrains Mono Italic", JETBRAINS_MONO_NERD_ITALIC),
        (
            "JetBrains Mono Bold Italic",
            JETBRAINS_MONO_NERD_BOLD_ITALIC,
        ),
        ("IBM Plex Sans Regular", IBM_PLEX_SANS_REGULAR),
        ("IBM Plex Sans SemiBold", IBM_PLEX_SANS_SEMIBOLD),
    ] {
        if let Err(error) = cx.text_system().add_fonts(vec![Cow::Borrowed(bytes)]) {
            tracing::warn!(%error, face, "Unable to register a bundled font");
        }
    }
}
