use crate::{Crabdash, NewWindow, OpenRepository, Quit, ReportIssue};
use gpui::*;
const REPOSITORY_URL: &str = "https://github.com/th0jensen/crabdash";
const ISSUES_URL: &str = "https://github.com/th0jensen/crabdash/issues";
fn open_window(cx: &mut App, minimised: bool) -> anyhow::Result<AnyWindowHandle> {
    if let Some(handle) = primary_window(cx) {
        return Ok(handle);
    }
    let options = super::window::options(cx, minimised);
    let handle = cx.open_window(options, |window, cx| {
        super::window::prepare(window, cx);
        let view = cx.new(|cx| Crabdash::new(cx));
        view.update(cx, |view, cx| {
            view.attach_dashboard_visibility(window, minimised, cx);
            #[cfg(target_os = "macos")]
            view.install_native_shell(window, cx);
        });
        let focus = view.read(cx).focus_handle.clone();
        window.focus(&focus);
        if minimised {
            window.on_next_frame(|window, cx| {
                // A second launch can request Show before the first frame.
                // Its deferred visibility update cancels the initial minimize.
                let still_hidden = window
                    .root::<Crabdash>()
                    .flatten()
                    .is_some_and(|view| !view.read(cx).dashboard_visibility.is_visible());
                if still_hidden {
                    window.minimize_window();
                }
            });
        }
        view
    })?;
    Ok(handle.into())
}

fn primary_window(cx: &App) -> Option<AnyWindowHandle> {
    cx.windows()
        .into_iter()
        .find(|handle| handle.downcast::<Crabdash>().is_some())
}

fn existing_or_new_window(
    cx: &mut App,
    handle: Option<AnyWindowHandle>,
) -> Option<AnyWindowHandle> {
    handle.or_else(|| match open_window(cx, false) {
        Ok(handle) => Some(handle),
        Err(error) => {
            tracing::error!(%error, "Unable to open Crabdash window");
            None
        }
    })
}

fn show_window(cx: &mut App, token: Option<String>) {
    let handle = primary_window(cx);
    if let Some(handle) = existing_or_new_window(cx, handle) {
        if let Err(error) = handle.update(cx, |_, window, cx| {
            super::window::activate_window(window, token.as_deref(), cx);
        }) {
            tracing::warn!(%error, "Unable to show retained Crabdash window");
        }
    }
    cx.activate(true);
}

fn show_preferences(cx: &mut App, token: Option<String>) {
    // Activate and dispatch on the same retained dashboard handle.
    let handle = primary_window(cx);
    if let Some(handle) = existing_or_new_window(cx, handle) {
        if let Err(error) = handle.update(cx, |_, window, cx| {
            super::window::activate_window(window, token.as_deref(), cx);
            window.dispatch_action(Box::new(crate::OpenPreferences), cx);
        }) {
            tracing::warn!(%error, "Could not open preferences from the tray");
        }
    }
    cx.activate(true);
}

pub fn run() -> anyhow::Result<()> {
    // Claim before GPUI creates any platform windows. The guard remains alive
    // through Application::run, including when the dashboard is hidden to tray.
    let instance = match super::instance::claim()? {
        super::instance::Claim::Primary(instance) => instance,
        super::instance::Claim::Forwarded => return Ok(()),
    };
    let requests = instance.requests.clone();
    let app = Application::new().with_assets(crate::features::machines::logos::Assets);
    app.on_reopen(|cx| show_window(cx, None));
    app.run(move |cx: &mut App| {
        let minimised = crate::desktop::startup::start_minimised().unwrap_or(false);
        if !minimised {
            cx.activate(true);
        }
        super::appearance::initialize(cx);
        crate::register_fonts(cx);
        Crabdash::bind_keys(cx);
        cx.bind_keys([KeyBinding::new(
            crate::desktop::menus::shortcut("cmd-q", "ctrl-q"),
            Quit,
            None,
        )]);
        cx.spawn(async move |cx| {
            while let Ok(token) = requests.recv().await {
                if cx.update(|cx| show_window(cx, token)).is_err() {
                    break;
                }
            }
        })
        .detach();
        if let Some(commands) = super::tray::start(cx) {
            cx.spawn(async move |cx| {
                while let Ok(command) = commands.recv().await {
                    if cx
                        .update(|cx| match command {
                            super::tray::TrayCommand::Show(token) => show_window(cx, token),
                            super::tray::TrayCommand::Preferences(token) => {
                                show_preferences(cx, token)
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
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &NewWindow, cx| {
            // Keep old action invocations compatible without opening another dashboard.
            show_window(cx, None);
        });
        cx.on_action(|_: &OpenRepository, cx| cx.open_url(REPOSITORY_URL));
        cx.on_action(|_: &ReportIssue, cx| cx.open_url(ISSUES_URL));
        super::menus::install(cx);
        if let Err(error) = open_window(cx, minimised) {
            tracing::error!(%error, "Unable to open Crabdash window");
            cx.quit();
        }
    });
    drop(instance);
    Ok(())
}
