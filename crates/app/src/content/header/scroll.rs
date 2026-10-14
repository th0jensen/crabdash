//! Reveal selected tabs without overriding manual scrolling or an in-progress drag.
use crate::layout::geometry::reveal_offset;
use crate::{app::Crabdash, features::workspaces::model::Tab};
use gpui::{Context, Pixels, ScrollHandle, SharedString, Window, point, px};

#[derive(PartialEq)]
struct Geometry {
    tabs: Vec<Tab>,
    active: Tab,
    widths: Vec<Pixels>,
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
    widths: &[Pixels],
    viewport: Pixels,
    window: &mut Window,
    cx: &mut Context<Crabdash>,
) -> ScrollHandle {
    let id = SharedString::from(format!("tab-scroll-{}-{pane}", app.workspaces.store.active));
    let state = window.use_keyed_state(id, cx, |_, _| State::default());
    let geometry = Geometry {
        tabs: tabs.to_vec(),
        active,
        widths: widths.to_vec(),
        viewport: viewport.max(px(0.0)),
    };
    let selection = app
        .workspaces
        .tab_reveal
        .filter(|(target, tab, _)| *target == pane && *tab == active)
        .map(|(_, _, sequence)| sequence);
    let dragging = cx.has_active_drag();
    state.update(cx, |state, _: &mut Context<State>| {
        if !dragging
            && (state.geometry.as_ref() != Some(&geometry)
                || selection.is_some_and(|sequence| state.selection != Some(sequence)))
        {
            if let Some(index) = tabs.iter().position(|tab| *tab == active) {
                // Use measured widths from this frame; item bounds still refer
                // to the preceding layout when titles or pane geometry change.
                let offset = reveal_offset(state.scroll.offset().x, widths, viewport, index);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveals_variable_width_tabs_after_selection_and_resize() {
        let widths = [px(92.0), px(80.0), px(110.0), px(86.0)];
        assert_eq!(reveal_offset(px(0.0), &widths, px(250.0), 2), px(-32.0));
        assert_eq!(reveal_offset(px(-32.0), &widths, px(200.0), 3), px(-168.0));
        assert_eq!(reveal_offset(px(-168.0), &widths, px(200.0), 0), px(0.0));
        assert_eq!(reveal_offset(px(-32.0), &widths, px(400.0), 2), px(-32.0));
    }

    #[test]
    fn keeps_visible_tabs_and_anchors_oversized_titles_to_their_start() {
        let widths = [px(92.0), px(300.0), px(110.0)];
        assert_eq!(reveal_offset(px(-92.0), &widths, px(400.0), 1), px(-92.0));
        assert_eq!(reveal_offset(px(0.0), &widths, px(200.0), 1), px(-92.0));
        assert_eq!(reveal_offset(px(-10.0), &widths, px(0.0), 2), px(-10.0));
        assert_eq!(reveal_offset(px(-10.0), &widths, px(200.0), 99), px(-10.0));
    }
}
