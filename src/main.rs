mod helpers;
mod models;
mod ui;

use gpui::prelude::*;
use gpui::*;

use crate::ui::app::Crabdash;

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        Crabdash::bind_keys(cx);

        cx.open_window(
            WindowOptions {
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
    });
}
