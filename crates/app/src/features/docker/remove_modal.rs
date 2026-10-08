use gpui::prelude::*;
use gpui::*;
use lucide_icons::Icon;
use services::docker::DockerAction;

use crate::app::Crabdash;
use crate::components::{
    common::{button, lucide_icon},
    style,
};
use crate::desktop::controls;

pub fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let Some(removal) = app.docker_removal.as_ref() else {
        return div();
    };
    let force = removal.force;
    div()
        .absolute().top_0().left_0().size_full().occlude()
        .bg(rgba(0x00000099))
        .flex().items_center().justify_center()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(crate::desktop::controls::modal(
            div().w(gpui::rems(420.0 / 16.0)).p(px(20.0))
                .bg(rgb(0x1C1C1E)).border_1().border_color(rgb(0x303030))
                .rounded(px(style::RADIUS)).flex().flex_col().gap(px(16.0))
                .child(div().text_size(gpui::rems(style::TEXT / 16.0)).text_color(white())
                    .font_weight(FontWeight::SEMIBOLD).child("Remove container?"))
                .child(div().flex().flex_col().gap(px(6.0))
                    .child(div().text_color(rgb(0xD4D4D4)).child(removal.name.clone()))
                    .child(div().text_size(gpui::rems(style::META / 16.0)).text_color(rgb(0x909090))
                        .child(format!("{} · {}", removal.machine_name, removal.id))))
                .child(div().text_size(gpui::rems(style::TEXT / 16.0)).text_color(rgb(0xAEAEB2))
                     .child(if force {
                        "The container will be force-removed immediately. Its writable layer and logs will be lost. Images, volumes and bind-mounted files are kept."
                    } else if removal.active {
                        "The container will be stopped, then removed. Its writable layer and logs will be lost. Images, volumes and bind-mounted files are kept."
                    } else {
                        "Its writable layer and logs will be lost. Images, volumes and bind-mounted files are kept."
                    }))
                .child(controls::selected_button(div().id("force-remove-container").flex().items_center().gap(px(8.0))
                    .cursor_pointer().text_color(if force { rgb(0xFF7068) } else { rgb(0x909090) })
                    .text_size(gpui::rems(style::META / 16.0))
                    .child(lucide_icon(if force { Icon::SquareCheck } else { Icon::Square }, style::ICON))
                    .child("Force removal (kills a running container)")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(removal) = this.docker_removal.as_mut() { removal.force = !removal.force; }
                        cx.notify();
                    })), "Force removal (kills a running container)", Some(if force { Icon::SquareCheck } else { Icon::Square }), force))
                .child(controls::action_group(div().flex().justify_end().gap(px(8.0)), vec![
                    button("cancel-remove-container", None::<Icon>, Some("Cancel"), false)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.docker_removal = None;
                            this.focus_handle.focus(window);
                            cx.notify();
                        })),
                    controls::destructive(button("confirm-remove-container", Some(Icon::Trash2), Some(if force { "Force remove" } else { "Remove" }), false))
                        .text_color(rgb(0xFF7068))
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(removal) = this.docker_removal.take() {
                                this.execute_docker_action(removal.machine_uuid, removal.id, DockerAction::Remove { force: removal.force }, cx);
                            }
                            this.focus_handle.focus(window);
                            cx.notify();
                        })),
                    ]))
            , 0.6
        ))
}
