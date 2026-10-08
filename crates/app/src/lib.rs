use gpui::actions;

pub mod app;
pub mod components;
pub mod content;
pub mod desktop;
pub mod features;
mod fonts;
pub(crate) mod layout;
pub use app::Crabdash;
pub(crate) use desktop::about::show_about_dialog;
pub use fonts::{
    JETBRAINS_MONO_NERD_BOLD, JETBRAINS_MONO_NERD_BOLD_ITALIC, JETBRAINS_MONO_NERD_ITALIC,
    JETBRAINS_MONO_NERD_REGULAR, register_fonts,
};

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
        DismissModal,
        DismissDockerLogModal,
        Hide,
        HideOthers,
        MinimizeWindow,
        OpenAddMachine,
        OpenPreferences,
        ToggleAppMenu,
        NewWindow,
        OpenRepository,
        ReportIssue,
        Quit,
        RefreshServices,
        ShowAll,
        ShowDocker,
        ShowDisks,
        ShowServices,
        ShowSystem,
        SubmitModal,
        ToggleFullScreen,
        ToggleSidebar,
        ToggleTerminal,
        ToggleWorkspaces,
        ZoomWindow
    ]
);
