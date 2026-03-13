use gpui::prelude::*;
use gpui::*;

use app::Crabdash;

actions!(crabdash, [Quit]);

const MIN_WINDOW_WIDTH: f32 = 980.0;
const MIN_WINDOW_HEIGHT: f32 = 680.0;

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
        |_window, cx| cx.new(|cx| Crabdash::new(cx)),
    )
    .expect("failed to open Crabdash window");
}

fn main() {
    let app = Application::new();
    app.on_reopen(|cx| open_window(cx));
    app.run(|cx: &mut App| {
        set_app_icon();
        cx.activate(true);
        app::register_fonts(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(vec![Menu {
            name: "Crabdash".into(),
            items: vec![MenuItem::action("Quit", Quit)],
        }]);
        Crabdash::bind_keys(cx);
        open_window(cx);
    });
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn set_app_icon() {
    use objc2::AnyThread;
    use objc2_app_kit::NSImage;
    use objc2_foundation::{MainThreadMarker, NSData};

    let icon_bytes = include_bytes!("../../../assets/icon.png");

    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        let data = NSData::with_bytes(icon_bytes);
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            objc2_app_kit::NSApplication::sharedApplication(mtm)
                .setApplicationIconImage(Some(&image));
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_app_icon() {}
