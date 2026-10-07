//! GPUI sends the shared menu definitions to AppKit's native menu bar.
use crate::{Hide, HideOthers, ShowAll, app::Crabdash};
use gpui::*;
pub(crate) fn visible(_: &Crabdash, _: &Window) -> bool {
    false
}
pub(crate) fn render(_: &Crabdash, _: &mut Context<Crabdash>) -> Div {
    div()
}
pub(crate) fn popup(_: &Crabdash, _: &mut Window, _: &mut Context<Crabdash>) -> AnyElement {
    div().into_any_element()
}
pub(super) fn intercept(_: &mut Context<Crabdash>) -> Option<Subscription> {
    None
}
pub(super) fn append_app_items(items: &mut Vec<MenuItem>) {
    items.push(MenuItem::os_submenu("Services", SystemMenuType::Services));
    items.push(MenuItem::separator());
    items.push(MenuItem::action("Hide Crabdash", Hide));
    items.push(MenuItem::action("Hide Others", HideOthers));
    items.push(MenuItem::action("Show All", ShowAll));
    items.push(MenuItem::separator());
}
pub(super) fn register_actions(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
    ]);
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
}
pub(super) fn shortcut(macos: &'static str, _linux: &'static str) -> &'static str {
    macos
}
