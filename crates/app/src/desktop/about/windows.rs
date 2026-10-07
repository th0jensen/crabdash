use crate::{APP_LICENSE, APP_NAME, APP_VERSION, app_authors_display, short_git_commit_hash};
pub(crate) fn show_about_dialog(window: &mut gpui::Window, cx: &mut gpui::App) {
    let detail = format!(
        "Version {} ({})\n{} · {}",
        APP_VERSION,
        short_git_commit_hash(),
        app_authors_display(),
        APP_LICENSE
    );
    let _response = window.prompt(
        gpui::PromptLevel::Info,
        APP_NAME,
        Some(&detail),
        &["Close"],
        cx,
    );
}
