//! A real glass container batches leaf effects; empty regions pass through to GPUI.
use super::leaf;
use gpui::{prelude::*, *};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectContainerView, NSGlassEffectView, NSView,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct Content;
    unsafe impl NSObjectProtocol for Content {}
    impl Content {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            // SAFETY: Standard NSView hit testing; discard only its empty surface.
            let result: Option<Retained<NSView>> = unsafe { msg_send![super(self), hitTest: point] };
            result.filter(|view| Retained::as_ptr(view).cast::<()>() != (self as *const Self).cast())
        }
    }
);
define_class!(
    #[unsafe(super = NSGlassEffectContainerView)]
    #[thread_kind = MainThreadOnly]
    struct Container;
    unsafe impl NSObjectProtocol for Container {}
    impl Container {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            // SAFETY: Let native descendants receive input, never the batching plane.
            let result: Option<Retained<NSView>> = unsafe { msg_send![super(self), hitTest: point] };
            result.filter(|view| Retained::as_ptr(view).cast::<()>() != (self as *const Self).cast())
        }
    }
);
define_class!(
    #[unsafe(super = NSGlassEffectView)]
    #[thread_kind = MainThreadOnly]
    struct Surface;
    unsafe impl NSObjectProtocol for Surface {}
    impl Surface {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            // SAFETY: Native child buttons own input; the shared glass has no action.
            let hit: Option<Retained<NSView>> = unsafe { msg_send![super(self), hitTest: point] };
            hit.filter(|view| Retained::as_ptr(view).cast::<()>() != (self as *const Self).cast())
        }
    }
);
#[derive(Default)]
struct Pools(HashMap<gpui::WindowId, Weak<Group>>);
impl Global for Pools {}
pub(super) struct Group {
    root: Retained<Container>,
    content: Retained<Content>,
    surface: Option<Retained<Surface>>,
}
impl Group {
    pub(super) fn get(
        renderer: &NSView,
        window: &Window,
        cx: &mut App,
        mtm: MainThreadMarker,
    ) -> Rc<Self> {
        let id = window.window_handle().window_id();
        if let Some(group) = cx
            .default_global::<Pools>()
            .0
            .get(&id)
            .and_then(Weak::upgrade)
        {
            return group;
        }
        let root: Retained<Container> =
            unsafe { msg_send![super(Container::alloc(mtm).set_ivars(())), init] };
        let content: Retained<Content> =
            unsafe { msg_send![super(Content::alloc(mtm).set_ivars(())), init] };
        root.setContentView(Some(&content));
        root.setSpacing(0.0);
        root.setFrame(renderer.bounds());
        root.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        content.setFrame(root.bounds());
        content.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        renderer.addSubview(&root);
        let group = Rc::new(Self {
            root,
            content,
            surface: None,
        });
        let pools = cx.default_global::<Pools>();
        pools.0.retain(|_, entry| entry.strong_count() != 0);
        pools.0.insert(id, Rc::downgrade(&group));
        group
    }
    pub(super) fn joined(&self) -> bool {
        self.surface.is_some()
    }
    pub(super) fn add(&self, view: &NSView) {
        // SAFETY: Both views are retained and accessed on the main thread;
        // the returned parent is retained while its identity is inspected.
        let parent = unsafe { view.superview() };
        if parent.as_ref().is_none_or(|parent| {
            Retained::as_ptr(parent) != (&*self.content as *const Content).cast()
        }) {
            self.content.addSubview(view);
        }
    }
    pub(super) fn rect(
        &self,
        rect: objc2_foundation::NSRect,
        renderer: &NSView,
    ) -> objc2_foundation::NSRect {
        self.content.convertRect_fromView(rect, Some(renderer))
    }
}
impl Drop for Group {
    fn drop(&mut self) {
        self.root.removeFromSuperview();
    }
}

struct Scope {
    window: WindowId,
    bounds: Bounds<Pixels>,
    group: Rc<Group>,
}
#[derive(Default)]
struct Scopes(Vec<Scope>);
impl Global for Scopes {}

pub(super) fn painting_group(
    bounds: Bounds<Pixels>,
    window: &Window,
    cx: &mut App,
) -> Option<Rc<Group>> {
    cx.default_global::<Scopes>()
        .0
        .last()
        .filter(|scope| {
            scope.window == Window::window_handle(window).window_id()
                && scope.bounds.intersect(&bounds) == bounds
        })
        .map(|scope| scope.group.clone())
}

// Each action remains a genuine NSButton. The container groups their glass
// sampling without replacing stock button metrics, behavior, or callbacks.
struct Row {
    group: Rc<Group>,
}
impl Row {
    fn new(window: &Window) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        // SAFETY: GPUI retains this live NSView for the calling window.
        let renderer = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let root: Retained<Container> =
            unsafe { msg_send![super(Container::alloc(mtm).set_ivars(())), init] };
        let content: Retained<Content> =
            unsafe { msg_send![super(Content::alloc(mtm).set_ivars(())), init] };
        // The shared native glass, rather than independent bordered bezels,
        // provides the joined action surface. Keep AppKit's default curvature.
        let surface: Retained<Surface> =
            unsafe { msg_send![super(Surface::alloc(mtm).set_ivars(())), init] };
        surface.setContentView(Some(&content));
        surface.setClipsToBounds(true);
        root.setContentView(Some(&surface));
        root.setSpacing(8.0);
        root.setClipsToBounds(true);
        renderer.addSubview(&root);
        Some(Self {
            group: Rc::new(Group {
                root,
                content,
                surface: Some(surface),
            }),
        })
    }
    fn position(&self, bounds: Bounds<Pixels>, opacity: f32, toolbar: bool, window: &Window) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: GPUI retains this live NSView for the calling window.
        let renderer = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let origin = renderer.bounds().origin;
        let y = if renderer.isFlipped() {
            origin.y + f64::from(f32::from(bounds.origin.y))
        } else {
            origin.y + renderer.bounds().size.height - f64::from(f32::from(bounds.bottom()))
        };
        self.group.root.setFrame(NSRect::new(
            NSPoint::new(origin.x + f64::from(f32::from(bounds.origin.x)), y),
            NSSize::new(
                f64::from(f32::from(bounds.size.width)),
                f64::from(f32::from(bounds.size.height)),
            ),
        ));
        if let Some(surface) = &self.group.surface {
            surface.setFrame(self.group.root.bounds());
            if toolbar {
                // Toolbar groups use a shared capsule, like NSToolbarItemGroup.
                surface.setCornerRadius(f64::from(f32::from(bounds.size.height)) / 2.0);
            }
            self.group.content.setFrame(surface.bounds());
        }
        self.group.root.setAlphaValue(f64::from(opacity));
        self.group.root.setHidden(false);
    }
}

pub(crate) struct Actions {
    inner: Stateful<Div>,
    toolbar: bool,
}
impl Actions {
    pub(crate) fn new(inner: Stateful<Div>) -> Self {
        Self {
            inner,
            toolbar: false,
        }
    }
    pub(crate) fn toolbar(inner: Stateful<Div>) -> Self {
        Self {
            inner,
            toolbar: true,
        }
    }
}
impl IntoElement for Actions {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Actions {
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
        (inner, window.insert_hitbox(bounds, HitboxBehavior::Normal))
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
        let Some(id) = id else {
            self.inner
                .paint(id, inspector, bounds, state, &mut prepaint.0, window, cx);
            return;
        };
        window.with_element_state(id, |old: Option<Option<Row>>, window| {
            let mut row = old.flatten();
            let presentation = window.native_control_presentation(&prepaint.1);
            let native =
                leaf::allowed(window, cx) && presentation != NativeControlPresentation::Hidden;
            let opacity = match presentation {
                NativeControlPresentation::Dimmed(alpha) => alpha,
                _ => 1.0,
            };
            if native {
                if row.is_none() {
                    row = Row::new(window);
                }
                if let Some(native_row) = row.as_ref() {
                    native_row.position(bounds, opacity, self.toolbar, window);
                    cx.default_global::<Scopes>().0.push(Scope {
                        window: Window::window_handle(window).window_id(),
                        bounds,
                        group: native_row.group.clone(),
                    });
                    self.inner.paint(
                        Some(id),
                        inspector,
                        bounds,
                        state,
                        &mut prepaint.0,
                        window,
                        cx,
                    );
                    cx.default_global::<Scopes>().0.pop();
                    return ((), row);
                }
            }
            if let Some(row) = row.as_ref() {
                row.group.root.setHidden(true);
            }
            self.inner.paint(
                Some(id),
                inspector,
                bounds,
                state,
                &mut prepaint.0,
                window,
                cx,
            );
            ((), row)
        });
    }
}
