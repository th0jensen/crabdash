//! Exercise real tooltip tasks/callbacks without creating an OS window.
use super::{
    ActiveTooltip, HOVERABLE_TOOLTIP_HIDE_DELAY, TOOLTIP_SHOW_DELAY, clear_active_tooltip,
    handle_tooltip_mouse_move,
};
use crate::prelude::*;
use crate::{
    self as gpui, AnyTooltip, AnyView, App, AppContext, Bounds, DispatchPhase, Empty, Modifiers,
    MouseExitEvent, Pixels, PlatformInput, TestAppContext, VisualTestContext, Window, point, px,
    size,
};
use crate::{Context, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, div};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

struct Harness {
    active: Rc<RefCell<Option<ActiveTooltip>>>,
    source: Rc<Cell<bool>>,
    builds: Rc<Cell<usize>>,
    builder: Rc<dyn Fn(&mut Window, &mut App) -> Option<(AnyView, bool)>>,
    hovered: Rc<dyn Fn(&Window) -> bool>,
}

impl Harness {
    fn new(hoverable: bool) -> Self {
        let source = Rc::new(Cell::new(true));
        let builds = Rc::new(Cell::new(0));
        let builder = {
            let builds = builds.clone();
            Rc::new(move |_: &mut Window, cx: &mut App| {
                builds.set(builds.get() + 1);
                Some((cx.new(|_| Empty).into(), hoverable))
            })
        };
        let hovered = {
            let source = source.clone();
            Rc::new(move |window: &Window| source.get() && window.tooltip_pointer_present())
        };
        Self {
            active: Default::default(),
            source,
            builds,
            builder,
            hovered,
        }
    }

    fn move_over_source(&self, cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            handle_tooltip_mouse_move(
                &self.active,
                &self.builder,
                &self.hovered,
                &self.hovered,
                DispatchPhase::Bubble,
                window,
                cx,
            )
        });
        cx.run_until_parked();
    }

    fn tooltip(&self) -> AnyTooltip {
        match self.active.borrow().as_ref() {
            Some(
                ActiveTooltip::Visible { tooltip, .. }
                | ActiveTooltip::WaitingForHide { tooltip, .. },
            ) => tooltip.clone(),
            _ => panic!("Expected a visible tooltip"),
        }
    }
}

fn advance(cx: &VisualTestContext, duration: Duration) {
    cx.executor().advance_clock(duration);
    cx.run_until_parked();
}

fn near_pointer() -> Bounds<Pixels> {
    Bounds::new(point(px(-10.0), px(-10.0)), size(px(100.0), px(100.0)))
}

#[gpui::test]
fn real_show_tasks_require_a_full_fresh_delay_after_rapid_exit_reentry(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let mut platform = cx.update(|window, _| {
        window
            .platform_window
            .as_test()
            .expect("Test backend")
            .clone()
    });
    platform.simulate_hover_status_change(true);
    let harness = Harness::new(false);
    harness.move_over_source(cx);
    advance(cx, Duration::from_millis(400));
    assert_eq!(harness.builds.get(), 0);
    platform.simulate_input(PlatformInput::MouseExited(MouseExitEvent {
        position: point(px(0.0), px(0.0)),
        pressed_button: None,
        modifiers: Modifiers::default(),
    }));
    platform.simulate_hover_status_change(false);
    platform.simulate_hover_status_change(true);
    harness.move_over_source(cx);
    advance(cx, Duration::from_millis(100));
    assert_eq!(
        harness.builds.get(),
        0,
        "Old show deadline cannot resurrect a tooltip"
    );
    advance(cx, TOOLTIP_SHOW_DELAY - Duration::from_millis(101));
    assert_eq!(harness.builds.get(), 0);
    advance(cx, Duration::from_millis(1));
    assert_eq!(harness.builds.get(), 1);
    let tooltip = harness.tooltip();
    platform.simulate_hover_status_change(false);
    assert!(!cx.update(|window, cx| (tooltip.check_visible_and_update)(
        near_pointer(),
        window,
        cx
    )));
    assert!(harness.active.borrow().is_none());
}

#[gpui::test]
fn cached_visible_request_cannot_render_a_new_waiting_show_in_the_same_epoch(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let platform = cx.update(|window, _| {
        window
            .platform_window
            .as_test()
            .expect("Test backend")
            .clone()
    });
    platform.simulate_hover_status_change(true);
    let harness = Harness::new(false);
    harness.move_over_source(cx);
    advance(cx, TOOLTIP_SHOW_DELAY);
    let cached = harness.tooltip();
    cx.update(|window, _| clear_active_tooltip(&harness.active, window));
    harness.move_over_source(cx);
    assert!(!cx.update(|window, cx| (cached.check_visible_and_update)(near_pointer(), window, cx)));
    assert!(matches!(
        harness.active.borrow().as_ref(),
        Some(ActiveTooltip::WaitingForShow { .. })
    ));
    advance(cx, TOOLTIP_SHOW_DELAY - Duration::from_millis(1));
    assert_eq!(harness.builds.get(), 1);
    advance(cx, Duration::from_millis(1));
    assert_eq!(harness.builds.get(), 2);
}

#[gpui::test]
fn hoverable_source_transition_and_hide_cancellation_survive_inside_the_renderer(
    cx: &mut TestAppContext,
) {
    let cx = cx.add_empty_window();
    let platform = cx.update(|window, _| {
        window
            .platform_window
            .as_test()
            .expect("Test backend")
            .clone()
    });
    // Inactive PopUp windows still support genuinely hovered tooltips.
    platform.simulate_hover_status_change(true);
    let harness = Harness::new(true);
    harness.move_over_source(cx);
    advance(cx, TOOLTIP_SHOW_DELAY);
    let tooltip = harness.tooltip();
    harness.source.set(false);
    assert!(cx.update(|window, cx| (tooltip.check_visible_and_update)(near_pointer(), window, cx)));
    assert!(matches!(
        harness.active.borrow().as_ref(),
        Some(ActiveTooltip::Visible { .. })
    ));
    let away = Bounds::new(point(px(1000.0), px(1000.0)), size(px(20.0), px(20.0)));
    assert!(cx.update(|window, cx| (tooltip.check_visible_and_update)(away, window, cx)));
    assert!(matches!(
        harness.active.borrow().as_ref(),
        Some(ActiveTooltip::WaitingForHide { .. })
    ));
    advance(cx, Duration::from_millis(200));
    assert!(cx.update(|window, cx| (tooltip.check_visible_and_update)(near_pointer(), window, cx)));
    advance(cx, HOVERABLE_TOOLTIP_HIDE_DELAY);
    assert!(matches!(
        harness.active.borrow().as_ref(),
        Some(ActiveTooltip::Visible { .. })
    ));
    platform.simulate_hover_status_change(false);
    assert!(!cx.update(|window, cx| (tooltip.check_visible_and_update)(
        near_pointer(),
        window,
        cx
    )));
    assert!(harness.active.borrow().is_none());
}

#[gpui::test]
fn focus_loss_cancels_pending_real_show_without_building(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let platform = cx.update(|window, _| {
        window
            .platform_window
            .as_test()
            .expect("Test backend")
            .clone()
    });
    platform.simulate_hover_status_change(true);
    platform.simulate_active_status_change(true);
    let harness = Harness::new(false);
    harness.move_over_source(cx);
    advance(cx, Duration::from_millis(100));
    platform.simulate_active_status_change(false);
    advance(cx, TOOLTIP_SHOW_DELAY);
    assert_eq!(harness.builds.get(), 0);
    assert!(harness.active.borrow().is_none());
}

#[gpui::test]
fn pending_exit_stays_hidden_after_the_original_show_deadline(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let platform = cx.update(|window, _| {
        window
            .platform_window
            .as_test()
            .expect("Test backend")
            .clone()
    });
    platform.simulate_hover_status_change(true);
    let harness = Harness::new(false);
    harness.move_over_source(cx);
    advance(cx, Duration::from_millis(100));
    platform.simulate_hover_status_change(false);
    advance(cx, TOOLTIP_SHOW_DELAY);
    assert_eq!(harness.builds.get(), 0);
    assert!(harness.active.borrow().is_none());
}

#[gpui::test]
fn hoverable_hide_delay_expires_inside_the_window_away_from_both_surfaces(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let platform = cx.update(|window, _| {
        window
            .platform_window
            .as_test()
            .expect("Test backend")
            .clone()
    });
    platform.simulate_hover_status_change(true);
    let harness = Harness::new(true);
    harness.move_over_source(cx);
    advance(cx, TOOLTIP_SHOW_DELAY);
    let tooltip = harness.tooltip();
    harness.source.set(false);
    let away = Bounds::new(point(px(1000.0), px(1000.0)), size(px(20.0), px(20.0)));
    assert!(cx.update(|window, cx| (tooltip.check_visible_and_update)(away, window, cx)));
    cx.run_until_parked();
    advance(cx, HOVERABLE_TOOLTIP_HIDE_DELAY - Duration::from_millis(1));
    assert!(matches!(
        harness.active.borrow().as_ref(),
        Some(ActiveTooltip::WaitingForHide { .. })
    ));
    advance(cx, Duration::from_millis(1));
    assert!(harness.active.borrow().is_none());
}

#[gpui::test]
fn pending_source_removal_without_motion_does_not_invoke_the_builder(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let platform = cx.update(|window, _| {
        window
            .platform_window
            .as_test()
            .expect("Test backend")
            .clone()
    });
    platform.simulate_hover_status_change(true);
    let harness = Harness::new(false);
    harness.move_over_source(cx);
    advance(cx, Duration::from_millis(100));
    harness.source.set(false);
    advance(cx, TOOLTIP_SHOW_DELAY);
    assert_eq!(harness.builds.get(), 0);
    assert!(harness.active.borrow().is_none());
}

struct SourceView {
    builds: Rc<Cell<usize>>,
    covered: bool,
}

impl Render for SourceView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let builds = self.builds.clone();
        div()
            .size(px(100.0))
            .relative()
            .child(div().id("source").size(px(100.0)).tooltip(move |_, cx| {
                builds.set(builds.get() + 1);
                cx.new(|_| Empty).into()
            }))
            .when(self.covered, |this| {
                this.child(div().absolute().inset_0().occlude())
            })
    }
}

#[gpui::test]
fn stationary_source_redraw_preserves_pending_show_with_its_new_hitbox_id(cx: &mut TestAppContext) {
    let builds = Rc::new(Cell::new(0));
    let counter = builds.clone();
    let (view, cx) = cx.add_window_view(|_, _| SourceView {
        builds: counter,
        covered: false,
    });
    let platform = cx.update(|window, _| {
        window
            .platform_window
            .as_test()
            .expect("Test backend")
            .clone()
    });
    platform.simulate_hover_status_change(true);
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.simulate_mouse_move(point(px(20.0), px(20.0)), None, Modifiers::default());
    advance(cx, Duration::from_millis(250));
    let before = cx.update(|window, _| {
        window
            .rendered_frame
            .hitboxes
            .first()
            .map(|hitbox| hitbox.id)
    });
    view.update(cx, |_, cx| cx.notify());
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    let after = cx.update(|window, _| {
        window
            .rendered_frame
            .hitboxes
            .first()
            .map(|hitbox| hitbox.id)
    });
    assert!(before.is_some() && after.is_some());
    assert_ne!(before, after, "The source must actually be rebuilt");
    advance(cx, Duration::from_millis(250));
    assert_eq!(builds.get(), 1);
}

#[gpui::test]
fn source_covered_during_a_pending_show_does_not_build_without_mouse_motion(
    cx: &mut TestAppContext,
) {
    let builds = Rc::new(Cell::new(0));
    let counter = builds.clone();
    let (view, cx) = cx.add_window_view(|_, _| SourceView {
        builds: counter,
        covered: false,
    });
    let platform = cx.update(|window, _| {
        window
            .platform_window
            .as_test()
            .expect("Test backend")
            .clone()
    });
    platform.simulate_hover_status_change(true);
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.simulate_mouse_move(point(px(20.0), px(20.0)), None, Modifiers::default());
    advance(cx, Duration::from_millis(250));
    view.update(cx, |view, cx| {
        view.covered = true;
        cx.notify();
    });
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    advance(cx, TOOLTIP_SHOW_DELAY);
    assert_eq!(builds.get(), 0);
}
