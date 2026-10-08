//! Workspace requests survive automatic clamps; only a completed drag changes them.
use crate::app::Crabdash;
use gpui::{prelude::*, *};
use uuid::Uuid;

pub(crate) fn valid_request(height: f32) -> bool {
    height.is_finite() && (1.0..=16_000.0).contains(&height)
}

fn requested(saved: Option<f32>, default: f32) -> f32 {
    saved.unwrap_or(default)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Token {
    pub id: Uuid,
    identity: Identity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    owner: EntityId,
    workspace: Uuid,
    machine: Uuid,
    scope: Uuid,
    model_revision: u64,
    store_revision: u64,
}

impl Token {
    pub fn new(app: &Crabdash, owner: EntityId) -> Option<Self> {
        let machine = app.selected_machine().uuid;
        let drawer = app.quake_terminals.get(&machine)?;
        Some(Self {
            id: Uuid::new_v4(),
            identity: Identity {
                owner,
                workspace: app.workspaces.store.active,
                machine,
                scope: drawer.model.scope,
                model_revision: drawer.model.revision,
                store_revision: app.workspaces.revision(),
            },
        })
    }

    pub fn valid(self, app: &Crabdash, owner: EntityId) -> bool {
        Some(self.identity) == Identity::current(app, owner)
    }
}

impl Identity {
    fn current(app: &Crabdash, owner: EntityId) -> Option<Self> {
        if app.quake_terminal_open
            && !app.preferences_open
            && app.machine_rename.target.is_none()
            && !app.add_machine_modal_open
            && !app.docker_run_modal_open
            && app.docker_removal.is_none()
            && !app.workspaces.open
            && app.open_menu.is_none()
        {
            let machine = app.selected_machine().uuid;
            let drawer = app.quake_terminals.get(&machine)?;
            Some(Self {
                owner,
                workspace: app.workspaces.store.active,
                machine,
                scope: drawer.model.scope,
                model_revision: drawer.model.revision,
                store_revision: app.workspaces.revision(),
            })
        } else {
            None
        }
    }
}

pub(crate) struct Draft {
    pub(super) token: Token,
    pub(super) down: Pixels,
    pub(super) initial: Pixels,
    pub(super) height: f32,
}

#[derive(Clone)]
pub(super) struct ResizeDrag {
    pub token: Token,
}

struct ResizePreview;
impl Render for ResizePreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_0()
    }
}

/// Child tab drags run first in GPUI's bubble phase; controls stop mousedown bubbling.
pub(super) fn resize_header(
    header: Stateful<Div>,
    app: &Crabdash,
    cx: &mut Context<Crabdash>,
) -> Stateful<Div> {
    let Some(token) = Token::new(app, cx.entity_id()) else {
        return header;
    };
    let owner = cx.entity();
    header
        .cursor_row_resize()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|app, event: &MouseDownEvent, _, _| {
                app.quake_resize_anchor = Some((event.position.y, app.quake_height));
            }),
        )
        .on_drag(ResizeDrag { token }, move |drag, _, window, cx| {
            owner.update(cx, |app, cx| {
                let anchor = app.quake_resize_anchor.take();
                if !drag.token.valid(app, cx.entity_id()) {
                    return;
                }
                if let Some((down, height)) = anchor {
                    app.begin_quake_resize(drag.token, down, height);
                    app.update_quake_resize(drag.token, window.mouse_position().y, window, cx);
                }
            });
            cx.new(|_| ResizePreview)
        })
}

fn finish(draft: &mut Option<Draft>, current: Option<Identity>) -> Option<f32> {
    let draft = draft.take()?;
    (Some(draft.token.identity) == current
        && valid_request(draft.height)
        && draft.height != f32::from(draft.initial))
    .then_some(draft.height)
}

impl Crabdash {
    pub(super) fn requested_quake_height(&self, window: &Window, cx: &App) -> f32 {
        if let Some(draft) = &self.quake_resize
            && draft.token.valid(self, draft.token.identity.owner)
        {
            return draft.height;
        }
        requested(
            self.workspaces.store.current().terminal_height,
            super::geometry::height_for_rows(
                self.preferences.terminal_rows,
                super::cell_metrics(&self.preferences, cx).1,
                crate::components::style::BAR * f32::from(window.rem_size()) / 16.0,
            ),
        )
    }

    pub(super) fn begin_quake_resize(&mut self, token: Token, down: Pixels, initial: Pixels) {
        self.quake_resize = Some(Draft {
            token,
            down,
            initial,
            height: f32::from(initial),
        });
    }

    pub(super) fn update_quake_resize(
        &mut self,
        token: Token,
        pointer: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !token.valid(self, cx.entity_id()) {
            return;
        }
        let Some(draft) = &self.quake_resize else {
            return;
        };
        if draft.token != token {
            return;
        }
        let height = super::geometry::dragged_height(
            f32::from(draft.initial),
            f32::from(draft.down),
            f32::from(pointer),
        );
        self.set_quake_height(px(height), window, cx);
    }

    /// Taking the draft makes release, release-out, and Escape idempotent.
    pub(crate) fn settle_quake_resize(&mut self, owner: EntityId) -> bool {
        self.quake_resize_anchor = None;
        let current = Identity::current(self, owner);
        let Some(height) = finish(&mut self.quake_resize, current) else {
            return false;
        };
        let saved = &mut self.workspaces.store.current_mut().terminal_height;
        let changed = *saved != Some(height);
        *saved = Some(height);
        changed
    }

    pub(crate) fn cancel_quake_resize(&mut self) {
        self.quake_resize = None;
        self.quake_resize_anchor = None;
    }

    pub(crate) fn reconcile_quake_resize(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.quake_resize.is_none() {
            // Mousedown can precede the drag threshold by several renders.
            return;
        }
        if self
            .quake_resize
            .as_ref()
            .is_some_and(|draft| !draft.token.valid(self, cx.entity_id()))
        {
            self.cancel_quake_resize();
        } else if !cx.has_active_drag() && self.settle_quake_resize(cx.entity_id()) {
            // A release outside the renderer can bypass its element listeners.
            self.persist_workspace(cx);
        }
    }
}

pub(crate) fn decorate(root: Div, app: &Crabdash, cx: &mut Context<Crabdash>) -> Div {
    let token = app.quake_resize.as_ref().map(|draft| draft.token);
    let finish = move |app: &mut Crabdash,
                       event: &MouseUpEvent,
                       _: &mut Window,
                       cx: &mut Context<Crabdash>| {
        if event.button == MouseButton::Left
            && token.is_some_and(|token| {
                app.quake_resize
                    .as_ref()
                    .is_some_and(|draft| draft.token == token)
            })
            && app.settle_quake_resize(cx.entity_id())
        {
            app.persist_workspace(cx);
        }
    };
    root.capture_any_mouse_up(cx.listener(finish))
        .on_mouse_up_out(MouseButton::Left, cx.listener(finish))
}

#[cfg(test)]
mod tests {
    use super::{Draft, Identity, Token, finish, requested};
    use crate::features::terminal::geometry::Geometry;
    use gpui::{EntityId, px};
    use uuid::Uuid;

    #[test]
    fn automatic_clamps_preserve_request_and_restore_it_after_growth() {
        let saved = Some(720.5);
        let geometry = |height| Geometry::new(900.0, height, 64.0, 8.0, 18.0);
        let small = geometry(400.0).clamp_height(requested(saved, 300.0));
        assert!(small < 720.5);
        assert_eq!(
            geometry(1000.0).clamp_height(requested(saved, 300.0)),
            720.5
        );
        assert_eq!(saved, Some(720.5));
    }

    #[test]
    fn only_default_height_follows_row_and_font_preferences() {
        let initial = super::super::geometry::height_for_rows(16, 18.0, 64.0);
        let updated = super::super::geometry::height_for_rows(24, 22.0, 80.0);
        assert_eq!(requested(None, initial), initial);
        assert_eq!(requested(None, updated), updated);
        assert_eq!(requested(Some(420.25), initial), 420.25);
        assert_eq!(requested(Some(420.25), updated), 420.25);
    }

    #[test]
    fn temporary_split_minimum_does_not_replace_requested_height() {
        let saved = Some(200.25);
        let model = super::super::model::Model::new(Uuid::new_v4());
        let split = model
            .with_session(Uuid::new_v4(), Some(crate::layout::Drop::Bottom))
            .expect("valid vertical split");
        let geometry = Geometry::new(900.0, 900.0, 64.0, 8.0, 18.0);
        let metrics = super::super::panes::Metrics::new(1.0, 18.0);
        let effective = super::super::panes::clamp_height(
            geometry,
            requested(saved, 300.0),
            Some(&split.layout.root),
            metrics,
        );
        assert!(effective > 200.25);
        assert_eq!(
            super::super::panes::clamp_height(
                geometry,
                requested(saved, 300.0),
                Some(&model.layout.root),
                metrics,
            ),
            200.25
        );
        assert_eq!(saved, Some(200.25));
    }

    fn identity() -> Identity {
        Identity {
            owner: EntityId::from(1),
            workspace: Uuid::new_v4(),
            machine: Uuid::new_v4(),
            scope: Uuid::new_v4(),
            model_revision: 2,
            store_revision: 3,
        }
    }

    fn draft(identity: Identity, height: f32) -> Option<Draft> {
        Some(Draft {
            token: Token {
                id: Uuid::new_v4(),
                identity,
            },
            down: px(300.0),
            initial: px(400.0),
            height,
        })
    }

    #[test]
    fn real_drag_finishes_once_while_click_and_duplicate_release_are_inert() {
        let current = identity();
        let mut pending = None;
        assert_eq!(finish(&mut pending, Some(current)), None);
        pending = draft(current, 400.0);
        let saved = Some(400.0);
        if let Some(draft) = &mut pending {
            // Motion changes the draft, leaving the committed request alone.
            draft.height = super::super::geometry::dragged_height(
                f32::from(draft.initial),
                f32::from(draft.down),
                250.5,
            );
        }
        assert_eq!(saved, Some(400.0));
        assert_eq!(finish(&mut pending, Some(current)), Some(449.5));
        // Release-out or Escape cannot finish the same draft twice.
        assert_eq!(finish(&mut pending, Some(current)), None);
    }

    #[test]
    fn stale_resize_owners_and_contexts_cannot_commit_to_another_workspace() {
        let initial = identity();
        let mut mismatches = Vec::new();
        let mut changed = initial;
        changed.owner = EntityId::from(2);
        mismatches.push(changed);
        changed = initial;
        changed.workspace = Uuid::new_v4();
        mismatches.push(changed);
        changed = initial;
        changed.machine = Uuid::new_v4();
        mismatches.push(changed);
        changed = initial;
        changed.scope = Uuid::new_v4();
        mismatches.push(changed);
        changed = initial;
        changed.model_revision += 1;
        mismatches.push(changed);
        changed = initial;
        changed.store_revision += 1;
        mismatches.push(changed);
        for current in mismatches {
            let mut pending = draft(initial, 500.0);
            assert_eq!(finish(&mut pending, Some(current)), None);
            assert!(pending.is_none());
        }
        assert_eq!(finish(&mut draft(initial, 500.0), None), None);
    }

    #[test]
    fn no_op_saturated_and_returned_drags_preserve_the_committed_request() {
        let current = identity();
        let mut pending = draft(current, 400.0);
        assert_eq!(finish(&mut pending, Some(current)), None);
        assert!(pending.is_none());
        // A saved 720px request is temporarily limited to 400px in this window.
        let geometry = Geometry::new(900.0, 480.0, 64.0, 8.0, 18.0);
        assert_eq!(geometry.clamp_height(requested(Some(720.0), 300.0)), 400.0);
        let mut saturated = draft(current, geometry.clamp_height(1600.0));
        assert_eq!(finish(&mut saturated, Some(current)), None);
        let mut pending = draft(current, 500.0);
        if let Some(draft) = &mut pending {
            draft.height = 400.0;
        }
        assert_eq!(finish(&mut pending, Some(current)), None);
        assert_eq!(
            finish(&mut draft(current, 399.5), Some(current)),
            Some(399.5)
        );
    }
}
