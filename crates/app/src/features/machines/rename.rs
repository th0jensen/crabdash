//! Persistent machine names are edited independently of connection identity.
use crate::{
    app::Crabdash,
    components::{common::button, style, text_field::TextField},
    desktop::controls,
};
use gpui::{prelude::*, *};
use lucide_icons::Icon;
use machines::{
    machine::validated_display_name,
    store::{MachineStore, load_store},
};
use uuid::Uuid;

pub(crate) struct Editor {
    pub(crate) target: Option<Uuid>,
    pub(crate) name: Entity<TextField>,
    pub(crate) busy: bool,
    error: Option<String>,
    focus_generation: u64,
}

impl Editor {
    pub(crate) fn new(cx: &mut Context<Crabdash>) -> Self {
        Self {
            target: None,
            name: cx.new(|cx| TextField::new("Name", "Machine name", 1, cx)),
            busy: false,
            error: None,
            focus_generation: 0,
        }
    }
}

impl Crabdash {
    pub(crate) fn open_machine_rename(
        &mut self,
        uuid: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.machine_rename.busy
            || self.preferences_open
            || self.add_machine_modal_open
            || self.docker_run_modal_open
            || self.docker_removal.is_some()
            || self.workspaces.open
        {
            return;
        }
        let Some(machine) = self
            .machine_store
            .machines
            .iter()
            .find(|machine| machine.uuid == uuid)
        else {
            return;
        };
        let name = machine.display_name().to_owned();
        self.machine_rename.target = Some(uuid);
        self.machine_rename.focus_generation = self.machine_rename.focus_generation.wrapping_add(1);
        self.open_menu = None;
        self.machine_rename.error = None;
        self.machine_rename.name.update(cx, |field, cx| {
            field.set_text(&name, cx);
            field.select_all_text(cx);
        });
        window.focus(&self.machine_rename.name.focus_handle(cx));
        let owner = cx.weak_entity();
        let focus_generation = self.machine_rename.focus_generation;
        window.on_next_frame(move |window, cx| {
            owner
                .update(cx, |this, cx| {
                    // The field joins the focus tree when the modal is drawn.
                    // Reassert focus after menu dismissal, only for this open.
                    if this.machine_rename.target == Some(uuid)
                        && this.machine_rename.focus_generation == focus_generation
                        && !this.machine_rename.busy
                    {
                        window.focus(&this.machine_rename.name.focus_handle(cx));
                    }
                })
                .ok();
        });
        cx.notify();
    }

    pub(crate) fn close_machine_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.machine_rename.busy {
            return;
        }
        self.machine_rename.target = None;
        self.machine_rename.error = None;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(crate) fn save_machine_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.machine_rename.busy {
            return;
        }
        let Some(uuid) = self.machine_rename.target else {
            return;
        };
        let name = match validated_display_name(&self.machine_rename.name.read(cx).text()) {
            Ok(name) => name,
            Err(error) => {
                self.machine_rename.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.machine_rename.busy = true;
        self.machine_rename.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this: WeakEntity<Crabdash>, cx| {
            let result = async {
                MachineStore::rename_machine(uuid, &name).await?;
                load_store().await
            }
            .await;
            this.update_in(cx, |this, window, cx| {
                this.machine_rename.busy = false;
                match result {
                    Ok(store) => {
                        let selected = this.selected_machine().uuid;
                        let mut store = store;
                        this.selected_machine =
                            super::model::reconcile(&mut store, &this.machine_store, selected);
                        this.machine_store = store;
                        this.close_machine_rename(window, cx);
                    }
                    Err(error) => {
                        this.machine_rename.error = Some(error.to_string());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }
}

pub(crate) fn render(app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let editor = &app.machine_rename;
    let busy = editor.busy;
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
        .bg(rgba(0x00000099))
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(controls::modal(
            div()
                .w(px(440.0))
                .p(px(20.0))
                .bg(rgb(style::CONTENT))
                .border_1()
                .border_color(rgb(style::BORDER))
                .rounded(px(style::RADIUS))
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(
                    div()
                        .text_color(rgb(style::TEXT_PRIMARY))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Rename machine"),
                )
                .child(editor.name.clone())
                .when_some(editor.error.as_ref(), |this, error| {
                    this.child(div().text_color(rgb(style::DANGER)).child(error.clone()))
                })
                .child(controls::action_group(
                    div().flex().justify_end().gap(px(8.0)),
                    vec![
                        button("cancel-machine-rename", None::<Icon>, Some("Cancel"), false)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_machine_rename(window, cx)
                            })),
                        controls::primary(button(
                            "save-machine-rename",
                            Some(Icon::Check),
                            Some(if busy { "Saving…" } else { "Save" }),
                            true,
                        ))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.save_machine_rename(window, cx)),
                        ),
                    ],
                )),
            0.6,
        ))
}
