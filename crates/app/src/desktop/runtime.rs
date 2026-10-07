use crate::{Crabdash, NewWindow, OpenRepository, Quit, ReportIssue};
use gpui::*;
const REPOSITORY_URL: &str = "https://github.com/th0jensen/crabdash";
const ISSUES_URL: &str = "https://github.com/th0jensen/crabdash/issues";
fn open_window(cx: &mut App, minimised: bool) {
    let options = super::window::options(cx, minimised);
    cx.open_window(options, |window, cx| {
        super::window::prepare(window, cx);
        let view = cx.new(|cx| Crabdash::new(cx));
        let focus = view.read(cx).focus_handle.clone();
        window.focus(&focus);
        if minimised {
            window.on_next_frame(|window, _| window.minimize_window());
        }
        view
    })
    .expect("failed to open Crabdash window");
}

fn show_window(cx: &mut App, token: Option<String>) {
    if let Some(handle) = cx.windows().first().copied() {
        let _ = handle.update(cx, |_, window, _| {
            super::window::activate_window(window, token.as_deref());
        });
    } else {
        open_window(cx, false);
    }
    cx.activate(true);
}

pub fn run() {
    let app = Application::new().with_assets(crate::features::machines::logos::Assets);
    app.on_reopen(|cx| show_window(cx, None));
    app.run(|cx: &mut App| {
        let minimised = crate::desktop::startup::start_minimised().unwrap_or(false);
        if !minimised {
            cx.activate(true);
        }
        crate::register_fonts(cx);
        Crabdash::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-shift-n", "ctrl-shift-n"),
                NewWindow,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-q", "ctrl-q"),
                Quit,
                None,
            ),
        ]);
        if let Some(commands) = super::tray::start(cx) {
            cx.spawn(async move |cx| {
                while let Ok(command) = commands.recv().await {
                    if cx
                        .update(|cx| match command {
                            super::tray::TrayCommand::Show(token) => show_window(cx, token),
                            super::tray::TrayCommand::Preferences => {
                                show_window(cx, None);
                                if let Some(handle) = cx.windows().first().copied() {
                                    if let Err(error) = handle.update(cx, |_, window, cx| window.dispatch_action(Box::new(crate::OpenPreferences), cx)) {
                                        tracing::warn!(%error, "Could not open preferences from the tray");
                                    }
                                }
                            }
                            super::tray::TrayCommand::Quit => cx.quit(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
        super::window::register_lifecycle(cx);
        cx.set_dock_menu(vec![MenuItem::action("New Window", NewWindow)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &NewWindow, cx| open_window(cx, false));
        cx.on_action(|_: &OpenRepository, cx| cx.open_url(REPOSITORY_URL));
        cx.on_action(|_: &ReportIssue, cx| cx.open_url(ISSUES_URL));
        super::menus::install(cx);
        open_window(cx, minimised);
    });
}
