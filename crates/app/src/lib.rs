use std::borrow::Cow;

use gpui::{App, actions};

mod about;
pub mod app;
pub mod components;
pub mod content;
pub(crate) use about::show_about_dialog;
pub use app::Crabdash;

pub const APP_NAME: &str = "Crabdash";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_COMMIT_HASH: &str = env!("CRABDASH_GIT_COMMIT_HASH");
pub const APP_ICON_PATH: &str = env!("CRABDASH_APP_ICON_PATH");
pub const APP_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
pub const APP_LICENSE: &str = env!("CARGO_PKG_LICENSE");
pub const SHORT_GIT_COMMIT_HASH_LENGTH: usize = 7;

pub fn short_git_commit_hash() -> &'static str {
    GIT_COMMIT_HASH
        .get(..SHORT_GIT_COMMIT_HASH_LENGTH)
        .unwrap_or(GIT_COMMIT_HASH)
}

pub fn app_authors_display() -> String {
    APP_AUTHORS.replace(':', ", ")
}

actions!(
    crabdash,
    [
        AboutCrabdash,
        CloseWindow,
        DismissAddMachineModal,
        DismissDockerLogModal,
        Hide,
        HideOthers,
        MinimizeWindow,
        OpenAddMachine,
        NewWindow,
        OpenRepository,
        ReportIssue,
        Quit,
        RefreshServices,
        ShowAll,
        SubmitAddMachineModal,
        ToggleFullScreen,
        ToggleSidebar,
        ZoomWindow
    ]
);

pub const JETBRAINS_MONO_NERD_REGULAR: &[u8] =
    include_bytes!("../assets/JetBrainsMonoNerdFont-Regular.ttf");
pub const JETBRAINS_MONO_NERD_BOLD: &[u8] =
    include_bytes!("../assets/JetBrainsMonoNerdFont-Bold.ttf");
pub const JETBRAINS_MONO_NERD_ITALIC: &[u8] =
    include_bytes!("../assets/JetBrainsMonoNerdFont-Italic.ttf");
pub const JETBRAINS_MONO_NERD_BOLD_ITALIC: &[u8] =
    include_bytes!("../assets/JetBrainsMonoNerdFont-BoldItalic.ttf");

pub fn register_fonts(cx: &mut App) {
    cx.text_system()
        .add_fonts(vec![
            Cow::Borrowed(lucide_icons::LUCIDE_FONT_BYTES),
            Cow::Borrowed(JETBRAINS_MONO_NERD_REGULAR),
            Cow::Borrowed(JETBRAINS_MONO_NERD_BOLD),
            Cow::Borrowed(JETBRAINS_MONO_NERD_ITALIC),
            Cow::Borrowed(JETBRAINS_MONO_NERD_BOLD_ITALIC),
        ])
        .expect("failed to load fonts");
}
