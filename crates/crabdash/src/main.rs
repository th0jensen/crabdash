use gpui::prelude::*;
use gpui::*;

use app::components::text_field::{FieldCopy, FieldCut, FieldPaste, FieldSelectAll};
use app::{
    AboutCrabdash, CloseWindow, Crabdash, Hide, HideOthers, MinimizeWindow, NewWindow,
    OpenAddMachine, OpenRepository, Quit, RefreshServices, ReportIssue, ShowAll, ToggleSidebar,
    ZoomWindow,
};

const MIN_WINDOW_WIDTH: f32 = 680.0;
const MIN_WINDOW_HEIGHT: f32 = 540.0;
const REPOSITORY_URL: &str = "https://github.com/th0jensen/crabdash";
const ISSUES_URL: &str = "https://github.com/th0jensen/crabdash/issues";

fn app_menus() -> Vec<Menu> {
    let mut app_items = Vec::new();

    app_items.push(MenuItem::action("About Crabdash", AboutCrabdash));
    app_items.push(MenuItem::separator());

    #[cfg(target_os = "macos")]
    {
        app_items.push(MenuItem::os_submenu("Services", SystemMenuType::Services));
        app_items.push(MenuItem::separator());
        app_items.push(MenuItem::action("Hide Crabdash", Hide));
        app_items.push(MenuItem::action("Hide Others", HideOthers));
        app_items.push(MenuItem::action("Show All", ShowAll));
        app_items.push(MenuItem::separator());
    }

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
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", MinimizeWindow),
                MenuItem::action("Zoom", ZoomWindow),
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

fn open_window(cx: &mut App) {
    cx.open_window(
        WindowOptions {
            window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))),
            titlebar: Some(TitlebarOptions {
                title: Some("Crabdash".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.0), px(11.0))),
                ..Default::default()
            }),
            ..WindowOptions::default()
        },
        |window, cx| {
            let view = cx.new(|cx| Crabdash::new(cx));
            let focus = view.read(cx).focus_handle.clone();
            window.focus(&focus);
            view
        },
    )
    .expect("failed to open Crabdash window");
}

fn main() {
    let app = Application::new();
    app.on_reopen(|cx| open_window(cx));
    app.run(|cx: &mut App| {
        cx.activate(true);
        app::register_fonts(cx);
        Crabdash::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-shift-n", NewWindow, None),
            KeyBinding::new("cmd-q", Quit, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-h", Hide, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("alt-cmd-h", HideOthers, None),
        ]);
        cx.set_dock_menu(vec![MenuItem::action("New Window", NewWindow)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &NewWindow, cx| open_window(cx));
        cx.on_action(|_: &OpenRepository, cx| cx.open_url(REPOSITORY_URL));
        cx.on_action(|_: &ReportIssue, cx| cx.open_url(ISSUES_URL));
        #[cfg(target_os = "macos")]
        cx.on_action(|_: &Hide, cx| cx.hide());
        #[cfg(target_os = "macos")]
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        #[cfg(target_os = "macos")]
        cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
        cx.set_menus(app_menus());
        open_window(cx);
    });
}
