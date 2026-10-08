//! GPUI layout and callbacks with real AppKit leaf content at paint time.
use super::{Spec, leaf};
use gpui::{prelude::*, *};

pub(crate) struct Control {
    inner: Stateful<Div>,
    spec: Spec,
    enabled: bool,
    constrained_width: bool,
    leading: bool,
}
impl Control {
    pub(crate) fn leading_identity(mut self) -> Self {
        self.constrained_width = true;
        self.leading = true;
        self
    }
    pub(crate) fn set_role(mut self, role: super::ButtonRole) -> Self {
        if let Spec::Button { role: value, .. } = &mut self.spec {
            *value = role;
        }
        self
    }
    pub(crate) fn set_choices(
        mut self,
        input: Entity<crate::components::text_field::TextField>,
        values: Vec<String>,
        help: SharedString,
    ) -> Self {
        self.spec = Spec::Popup {
            input,
            values,
            help,
        };
        self
    }
    pub(crate) fn set_selected(mut self, selected: bool) -> Self {
        if let Spec::Button {
            selected: value, ..
        } = &mut self.spec
        {
            *value = Some(selected);
        }
        self
    }
    pub(crate) fn set_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub(crate) fn new(inner: Stateful<Div>, spec: Spec) -> Self {
        Self {
            inner,
            spec,
            enabled: true,
            constrained_width: false,
            leading: false,
        }
    }
}
impl Styled for Control {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}
impl InteractiveElement for Control {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.inner.interactivity()
    }
}
impl StatefulInteractiveElement for Control {}
impl ParentElement for Control {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}
impl IntoElement for Control {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Control {
    type RequestLayoutState = <Stateful<Div> as Element>::RequestLayoutState;
    type PrepaintState = (Option<Hitbox>, Hitbox);
    fn id(&self) -> Option<ElementId> {
        Element::id(&self.inner)
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        if leaf::allowed(window, cx) {
            // AppKit supplies help for native controls; a second GPUI tooltip
            // would occlude the native control and swap its appearance.
            self.inner.interactivity().clear_tooltip();
            if let Some(size) = super::measure::size(&self.spec, window, cx) {
                self.inner.style().min_size.height = Some(px(size.height as f32).into());
                if self.constrained_width {
                    // Stock ideal width may shrink within the domain's column;
                    // it is not an intrinsic minimum that can overlap actions.
                    self.inner.style().size.width = Some(px(size.width as f32).into());
                } else if !matches!(self.spec, Spec::Search { .. }) {
                    self.inner.style().min_size.width = Some(px(size.width as f32).into());
                }
            }
        }
        self.inner.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let inner = self
            .inner
            .prepaint(id, inspector, bounds, state, window, cx);
        // A marker after our own children excludes the control and its enclosing
        // modal from later-layer occlusion checks.
        let marker = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        (inner, marker)
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let enabled = self.enabled
            && self
                .inner
                .style()
                .opacity
                .is_none_or(|opacity| opacity >= 1.0);
        let native = id.is_some_and(|id| {
            window.with_element_state(id, |old: Option<Option<leaf::Leaf>>, window| {
                let mut leaf = old.flatten().filter(|leaf| leaf.matches(&self.spec));
                let presentation = window.native_control_presentation(&prepaint.1);
                let visible =
                    leaf::allowed(window, cx) && presentation != NativeControlPresentation::Hidden;
                let interactive = enabled
                    && !cx.has_active_drag()
                    && presentation == NativeControlPresentation::Visible;
                let opacity = match presentation {
                    NativeControlPresentation::Dimmed(alpha) => alpha,
                    _ => 1.0,
                };
                if visible {
                    if leaf.is_none() {
                        leaf = leaf::Leaf::new(&self.spec, window, cx);
                    }
                    if let Some(leaf) = leaf.as_mut() {
                        leaf.synchronize(
                            &self.spec,
                            bounds,
                            interactive,
                            opacity,
                            self.leading,
                            window,
                            cx,
                        );
                    }
                } else if !leaf::allowed(window, cx) {
                    leaf = None;
                } else if let Some(leaf) = leaf.as_mut() {
                    leaf.hide(window, cx);
                }
                let painted = leaf.as_ref().is_some_and(leaf::Leaf::is_visible);
                (painted, leaf)
            })
        });
        if native {
            window.paint_native_control_fallback(|window| {
                self.inner
                    .paint(id, inspector, bounds, state, &mut prepaint.0, window, cx)
            });
        } else {
            self.inner
                .paint(id, inspector, bounds, state, &mut prepaint.0, window, cx);
        }
    }
}
