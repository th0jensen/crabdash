#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
mod keymap;
use crate::components::text_field::{FieldCopy, FieldCut, FieldPaste, FieldSelectAll};
use crate::{
    AboutCrabdash, CloseWindow, MinimizeWindow, NewWindow, OpenAddMachine, OpenPreferences,
    OpenRepository, Quit, RefreshServices, ReportIssue, ToggleFullScreen, ToggleSidebar,
    ToggleTerminal, ZoomWindow,
};
use gpui::*;
pub(crate) use platform::{popup, render, visible};
pub(crate) fn intercept(cx: &mut Context<crate::Crabdash>) -> Option<Subscription> {
    platform::intercept(cx)
}
pub(crate) fn shortcut(macos: &'static str, linux: &'static str) -> &'static str {
    platform::shortcut(macos, linux)
}
pub(super) fn install(cx: &mut App) {
    platform::register_actions(cx);
    cx.set_menus(app_menus());
}
pub(super) fn app_menus() -> Vec<Menu> {
    let mut app_items = Vec::new();

    app_items.push(MenuItem::action("About Crabdash", AboutCrabdash));
    app_items.push(MenuItem::action("Preferences…", OpenPreferences));
    app_items.push(MenuItem::separator());

    platform::append_app_items(&mut app_items);

    app_items.push(MenuItem::action("Quit Crabdash", Quit));

    vec![
        Menu {
            name: "Crabdash".into(),
            items: app_items,
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Add New Machine", OpenAddMachine),
                MenuItem::separator(),
                MenuItem::action("New Window", NewWindow),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Cut", FieldCut, OsAction::Cut),
                MenuItem::os_action("Copy", FieldCopy, OsAction::Copy),
                MenuItem::os_action("Paste", FieldPaste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action("Select All", FieldSelectAll, OsAction::SelectAll),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Refresh", RefreshServices),
                MenuItem::separator(),
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::action("Toggle Terminal", ToggleTerminal),
                MenuItem::separator(),
                MenuItem::action("Docker", crate::ShowDocker),
                MenuItem::action("Disks", crate::ShowDisks),
                MenuItem::action("Services", crate::ShowServices),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", MinimizeWindow),
                MenuItem::action("Zoom", ZoomWindow),
                MenuItem::action("Full Screen", ToggleFullScreen),
                MenuItem::separator(),
            ],
        },
        Menu {
            name: "Help".into(),
            items: vec![
                MenuItem::action("Crabdash Repository", OpenRepository),
                MenuItem::action("Report an Issue", ReportIssue),
            ],
        },
    ]
}
