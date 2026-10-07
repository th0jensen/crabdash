//! Reveal selected tabs without overriding manual scrolling or an in-progress drag.
use crate::{app::Crabdash, features::workspaces::model::Tab};
use gpui::{Context, Pixels, ScrollHandle, SharedString, Window, point, px};

#[derive(PartialEq)]
struct Geometry {
    tabs: Vec<Tab>,
    active: Tab,
    width: Pixels,
    viewport: Pixels,
}

#[derive(Default)]
struct State {
    scroll: ScrollHandle,
    geometry: Option<Geometry>,
    selection: Option<u64>,
}

pub(super) fn handle(
    app: &Crabdash,
    pane: u32,
    tabs: &[Tab],
    active: Tab,
    width: Pixels,
    viewport: Pixels,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> ScrollHandle {
    let id = SharedString::from(format!(
        "tab-scroll-{}-{pane}",
        app.workspaces.store.active
    ));
    let state = window.use_keyed_state(id, cx, |_, _| State::default());
    let geometry = Geometry {
        tabs: tabs.to_vec(),
        active,
        width,
        viewport: viewport.max(px(0.0)),
    };
    let selection = app
        .workspaces
        .tab_reveal
        .filter(|(tab, _)| *tab == active)
        .map(|(_, sequence)| sequence);
    let dragging = cx.has_active_drag();
    state.update(cx, |state, _: &mut Context<State>| {
        if !dragging
            && (state.geometry.as_ref() != Some(&geometry)
                || selection.is_some_and(|sequence| state.selection != Some(sequence)))
        {
            if let Some(index) = tabs.iter().position(|tab| *tab == active) {
                // Equal-width tabs let us use this frame's geometry. GPUI's
                // scroll_to_item still has default/previous bounds at this point.
                let offset = reveal_offset(state.scroll.offset().x, width, viewport, index);
                state.scroll.set_offset(point(offset, px(0.0)));
            }
            state.geometry = Some(geometry);
            if selection.is_some() {
                state.selection = selection;
            }
        }
        state.scroll.clone()
    })
}

fn reveal_offset(current: Pixels, width: Pixels, viewport: Pixels, index: usize) -> Pixels {
    if viewport <= px(0.0) {
        return current;
    }
    let left = width * index as f32;
    let right = left + width;
    if left + current < px(0.0) || width > viewport {
        -left
    } else if right + current > viewport {
        viewport - right
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveals_clipped_tabs_using_the_current_pane_width() {
        assert_eq!(reveal_offset(px(0.0), px(154.0), px(430.0), 2), px(-32.0));
        assert_eq!(reveal_offset(px(-32.0), px(154.0), px(330.0), 2), px(-132.0));
        assert_eq!(reveal_offset(px(-132.0), px(154.0), px(330.0), 0), px(0.0));
        assert_eq!(reveal_offset(px(0.0), px(220.0), px(858.0), 3), px(-22.0));
    }

    #[test]
    fn preserves_visible_tabs_and_prefers_identity_when_a_tab_cannot_fit() {
        assert_eq!(reveal_offset(px(-100.0), px(154.0), px(430.0), 1), px(-100.0));
        assert_eq!(reveal_offset(px(-32.0), px(154.0), px(600.0), 2), px(-32.0));
        assert_eq!(reveal_offset(px(0.0), px(300.0), px(200.0), 1), px(-300.0));
        assert_eq!(reveal_offset(px(-10.0), px(154.0), px(0.0), 2), px(-10.0));
    }
}
