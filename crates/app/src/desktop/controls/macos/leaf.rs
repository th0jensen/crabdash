//! Main-thread AppKit leaf ownership and asynchronous GPUI callback bridge.
use super::super::events::{self, Kind, Stamp, Sync};
use super::{
    Event, Spec,
    group::Group,
    native::{Content, Host, Passive, Target},
};
use crate::{
    app::Crabdash,
    components::text_field::TextField,
    features::{polling::Target as MachineTarget, workspaces::model::Node},
};
use gpui::*;
use objc2::{
    MainThreadMarker, MainThreadOnly, msg_send, rc::Retained, runtime::AnyClass,
    runtime::ProtocolObject, sel,
};
use objc2_app_kit::{NSColor, NSTextAlignment, NSTintProminence, NSView, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Copy, Eq, PartialEq)]
struct Surface {
    overlays: [bool; 6],
    machine: Option<uuid::Uuid>,
    workspace: uuid::Uuid,
    visible: u8,
}
impl Surface {
    fn same_editor(self, other: Self) -> bool {
        self.overlays == other.overlays
            && self.machine == other.machine
            && self.workspace == other.workspace
    }
}
fn same_surface(previous: Option<Surface>, current: Option<Surface>, editor: bool) -> bool {
    match (previous, current) {
        (Some(previous), Some(current)) if editor => previous.same_editor(current),
        _ => previous == current,
    }
}
fn surface(window: &Window, cx: &App) -> Option<Surface> {
    let owner = window.root::<Crabdash>().flatten()?;
    let app = owner.read(cx);
    Some(Surface {
        overlays: [
            app.preferences_open,
            app.add_machine_modal_open,
            app.docker_run_modal_open,
            app.docker_removal.is_some(),
            app.open_menu.is_some(),
            app.workspaces.open,
        ],
        machine: app
            .machine_store
            .machines
            .get(app.selected_machine)
            .map(|machine| machine.uuid),
        workspace: app.workspaces.store.active,
        visible: visible(&app.workspaces.store.current().layout.root),
    })
}
fn visible(node: &Node) -> u8 {
    match node {
        Node::Pane { active, .. } => 1 << (*active as u8),
        Node::Split { first, second, .. } => visible(first) | visible(second),
    }
}
fn current_target(window: &Window, cx: &App) -> Option<MachineTarget> {
    let owner = window.root::<Crabdash>().flatten()?;
    let app = owner.read(cx);
    app.machine_store
        .machines
        .get(app.selected_machine)
        .map(MachineTarget::from_machine)
}
fn same_target(token: &Token, window: &Window, cx: &App) -> bool {
    target_matches(token.target.borrow().as_ref(), window, cx)
}
fn target_matches(snapshot: Option<&MachineTarget>, window: &Window, cx: &App) -> bool {
    let Some(owner) = window.root::<Crabdash>().flatten() else {
        return false;
    };
    let app = owner.read(cx);
    match (
        snapshot,
        app.machine_store.machines.get(app.selected_machine),
    ) {
        (Some(snapshot), Some(machine)) => snapshot.matches(machine),
        (None, None) => true,
        _ => false,
    }
}
pub(super) fn allowed(window: &Window, cx: &App) -> bool {
    window
        .root::<Crabdash>()
        .flatten()
        .is_some_and(|owner| owner.read(cx).preferences.liquid_glass)
        && AnyClass::get(c"NSGlassEffectView").is_some()
}
fn views(window: &Window) -> Option<(&NSView, &NSWindow)> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: The live GPUI window owns this renderer NSView throughout this call.
    let renderer = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let native = renderer.window()?;
    // The returned native window is hierarchy-owned while renderer remains live.
    let native = unsafe { &*Retained::as_ptr(&native) };
    Some((renderer, native))
}
pub(super) struct Token {
    pub(super) revision: Cell<u64>,
    pub(super) sequence: Cell<u64>,
    pub(super) editing_revision: Cell<u64>,
    accepted_native: RefCell<Option<String>>,
    model_revision: Cell<u64>,
    visible: Cell<bool>,
    enabled: Cell<bool>,
    bounds: Cell<Bounds<Pixels>>,
    surface: Cell<Option<Surface>>,
    target: RefCell<Option<MachineTarget>>,
}
impl Token {
    pub(super) fn stamp(&self) -> Stamp {
        Stamp {
            geometry: self.revision.get(),
            editing: self.editing_revision.get(),
            sequence: self.sequence.get(),
            model: self.model_revision.get(),
        }
    }
}
pub(super) struct Leaf {
    root: Retained<NSView>,
    host: Option<Retained<Host>>,
    glass: Option<Retained<NSView>>,
    status_container: Option<Retained<NSView>>,
    content: Content,
    _target: Retained<Target>,
    token: Rc<Token>,
    _group: Rc<Group>,
    last_value: Option<String>,
    input_id: Option<gpui::EntityId>,
    initial: Spec,
    input: Option<WeakEntity<TextField>>,
    window: AnyWindowHandle,
    cx: AsyncApp,
}
impl Leaf {
    pub(super) fn new(spec: &Spec, window: &mut Window, cx: &mut App) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let (renderer, _) = views(window)?;
        let content = Content::new(spec, mtm);
        let status_container = if matches!(spec, Spec::Status { .. }) {
            let wrapper: Retained<Passive> =
                unsafe { msg_send![super(Passive::alloc(mtm).set_ivars(())), init] };
            wrapper.addSubview(content.view());
            wrapper.setClipsToBounds(true);
            Some(Retained::into_super(wrapper))
        } else {
            None
        };
        let glass = if !matches!(spec, Spec::Status { .. }) {
            None
        } else {
            let class = AnyClass::get(c"NSGlassEffectView")?;
            // SAFETY: Runtime-gated public macOS 26 class, initialized as an NSView.
            let view: Retained<NSView> = unsafe { msg_send![class, new] };
            unsafe {
                let _: () = msg_send![&view, setContentView: status_container.as_deref()];
            }
            Some(view)
        };
        let hosted = glass.as_deref().unwrap_or(content.view());
        let host = (!matches!(spec, Spec::Status { .. })).then(|| Host::new(mtm));
        let root = if matches!(spec, Spec::Status { .. }) {
            let allocated = Passive::alloc(mtm).set_ivars(());
            let passive: Retained<Passive> = unsafe { msg_send![super(allocated), init] };
            Retained::into_super(passive)
        } else if let Some(host) = &host {
            Retained::into_super(host.clone())
        } else {
            return None;
        };
        root.addSubview(hosted);
        root.setClipsToBounds(true);
        let group = Group::get(renderer, window, cx, mtm);
        group.add(&root);
        root.setHidden(true);
        let token = Rc::new(Token {
            revision: Cell::new(0),
            sequence: Cell::new(0),
            editing_revision: Cell::new(0),
            accepted_native: RefCell::new(None),
            model_revision: Cell::new(match spec {
                Spec::Search { input, .. } | Spec::Popup { input, .. } => {
                    input.read(cx).native_model_revision()
                }
                _ => 0,
            }),
            visible: Cell::new(false),
            enabled: Cell::new(true),
            bounds: Cell::new(Bounds::default()),
            surface: Cell::new(None),
            target: RefCell::new(current_target(window, cx)),
        });
        let (sender, receiver) = smol::channel::unbounded();
        let choices = match spec {
            Spec::Popup { values, .. } => Some(values.clone()),
            _ => None,
        };
        let target = Target::new(sender, token.clone(), choices, mtm);
        if !matches!(spec, Spec::Status { .. }) {
            // SAFETY: Target is retained by Leaf and accepts the NSControl sender.
            unsafe {
                content.control().setTarget(Some(&target));
                content.control().setAction(Some(sel!(crabdashControl:)));
            }
        }
        if let Content::Search(field) = &content {
            // Search delegate/target properties are weak; Leaf retains their owner.
            unsafe {
                field.setDelegate(Some(ProtocolObject::from_ref(&*target)));
            }
        }
        let weak = Rc::downgrade(&token);
        let handle = Window::window_handle(window);
        let input: Option<gpui::WeakEntity<TextField>> = match spec {
            Spec::Search { input, .. } | Spec::Popup { input, .. } => Some(input.downgrade()),
            _ => None,
        };
        let receiver_input = input.clone();
        let receiver_popup = matches!(spec, Spec::Popup { .. });
        cx.spawn(async move |cx| {
            while let Ok(event) = receiver.recv().await {
                let Some(token) = weak.upgrade() else {
                    break;
                };
                let _ = handle.update(cx, |_, window, cx| {
                    if !token.visible.get()
                        || !token.enabled.get()
                        || !allowed(window, cx)
                        || !same_surface(
                            token.surface.get(),
                            surface(window, cx),
                            receiver_input.is_some() && !receiver_popup,
                        )
                        || !same_target(&token, window, cx)
                        || cx.has_active_drag()
                    {
                        return;
                    }
                    let bounds = token.bounds.get();
                    let viewport = Bounds::new(Point::default(), window.viewport_size());
                    if bounds.size.width <= px(0.0)
                        || bounds.size.height <= px(0.0)
                        || bounds.intersect(&viewport) != bounds
                    {
                        return;
                    }
                    let (stamp, text) = match event {
                        Event::Action { stamp, text } => (stamp, Some(text)),
                        Event::Focus { stamp } => (stamp, None),
                    };
                    if receiver_input.is_none()
                        && !events::accepts(
                            token.stamp(),
                            stamp,
                            Kind::Click,
                            token.model_revision.get(),
                        )
                    {
                        return;
                    }
                    if let Some(input) = receiver_input.as_ref() {
                        let Some(input) = input.upgrade() else {
                            return;
                        };
                        let focus = text.is_none();
                        let kind = if receiver_popup {
                            Kind::Choice
                        } else if focus {
                            Kind::Focus
                        } else {
                            Kind::Edit
                        };
                        let model_revision = input.read(cx).native_model_revision();
                        if !events::accepts(token.stamp(), stamp, kind, model_revision) {
                            return;
                        }
                        if focus {
                            // Mirror pane capture/focus without changing AppKit's
                            // live first responder or its field-editor selection.
                            window.dispatch_native_control_click(bounds.center(), cx);
                        }
                        if receiver_popup {
                            if let Some((renderer, native)) = views(window) {
                                native.makeFirstResponder(Some(renderer));
                            }
                        }
                        let _ = input.update(cx, |field, cx| {
                            window.focus(&field.focus_handle(cx));
                            if let Some(text) = text.as_ref() {
                                if receiver_popup {
                                    field.set_text(text, cx);
                                    token.model_revision.set(field.native_model_revision());
                                } else {
                                    token.accepted_native.replace(Some(text.clone()));
                                    if field.text() != *text {
                                        field.set_native_text(text, cx);
                                    }
                                }
                            }
                        });
                    } else if text.is_some() {
                        // Let the existing GPUI listeners own all domain behavior.
                        // Only this still-visible exact element/window can dispatch.
                        if let Some((renderer, native)) = views(window) {
                            native.makeFirstResponder(Some(renderer));
                        }
                        window.dispatch_native_control_click(bounds.center(), cx);
                    }
                });
            }
        })
        .detach();
        Some(Self {
            root,
            host,
            glass,
            status_container,
            content,
            _target: target,
            token,
            _group: group,
            last_value: None,
            initial: spec.clone(),
            input_id: match spec {
                Spec::Search { input, .. } | Spec::Popup { input, .. } => Some(input.entity_id()),
                _ => None,
            },
            input,
            window: handle,
            cx: cx.to_async(),
        })
    }
    pub(super) fn matches(&self, spec: &Spec) -> bool {
        match (&self.content, spec) {
            (Content::Search(_), Spec::Search { input, placeholder }) => {
                self.input_id == Some(input.entity_id())
                    && matches!(&self.initial, Spec::Search { placeholder: old, .. } if old == placeholder)
            }
            (
                Content::Popup(_),
                Spec::Popup {
                    input,
                    values,
                    help,
                },
            ) => {
                self.input_id == Some(input.entity_id())
                    && matches!(&self.initial, Spec::Popup { values: old, help: old_help, .. } if old == values && old_help == help)
            }
            (
                Content::Button(_),
                Spec::Button {
                    icon,
                    selected,
                    role,
                    ..
                },
            ) => {
                matches!(&self.initial, Spec::Button { icon: old, selected: previous, role: old_role, .. } if old.as_ref().map(std::mem::discriminant) == icon.as_ref().map(std::mem::discriminant) && previous.is_some() == selected.is_some() && matches!(old_role, super::ButtonRole::Toolbar) == matches!(role, super::ButtonRole::Toolbar))
            }
            (Content::Switch(_), Spec::Switch { .. })
            | (Content::Status(_), Spec::Status { .. }) => true,
            _ => false,
        }
    }
    pub(super) fn is_visible(&self) -> bool {
        self.token.visible.get()
    }
    pub(super) fn hide(&mut self, _: &Window, _: &mut App) {
        if self.token.visible.replace(false) {
            self.token
                .revision
                .set(self.token.revision.get().wrapping_add(1));
            self.token
                .editing_revision
                .set(self.token.editing_revision.get().wrapping_add(1));
        }
        self.finish_editing();
        self.root.setHidden(true);
    }

    fn finish_editing(&self) {
        let Content::Search(field) = &self.content else {
            return;
        };
        if field.currentEditor().is_none() {
            return;
        }
        let field = field.clone();
        let text = field.stringValue().to_string();
        let model_revision = self.token.model_revision.get();
        let editor_surface = self.token.surface.get();
        let editor_target = self.token.target.borrow().clone();
        let input = self.input.clone();
        let handle = self.window;
        let token = Rc::downgrade(&self.token);
        self.cx
            .spawn(async move |cx| {
                let _ = handle.update(cx, |_, window, cx| {
                    if token
                        .upgrade()
                        .is_some_and(|token| token.visible.get() && token.enabled.get())
                    {
                        return;
                    }
                    // Teardown can outlive its leaf; retain the same context
                    // guards as ordinary editor events without retaining it.
                    let same_context = same_surface(editor_surface, surface(window, cx), true)
                        && target_matches(editor_target.as_ref(), window, cx);
                    if let Some(input) = input.filter(|_| same_context) {
                        let _ = input.update(cx, |input, cx| {
                            // A deliberate model replacement takes precedence over
                            // final text from a disappearing native editor.
                            if input.native_model_revision() == model_revision
                                && input.text() != text
                            {
                                input.set_native_text(&text, cx);
                            }
                        });
                    }
                    if let (Some(editor), Some((renderer, native))) =
                        (field.currentEditor(), views(window))
                    {
                        // The shared AppKit field editor may now belong to a
                        // different search field; query this field's ownership.
                        if native.firstResponder().as_ref().is_some_and(|responder| {
                            Retained::as_ptr(responder).cast::<()>()
                                == Retained::as_ptr(&editor).cast::<()>()
                        }) {
                            native.makeFirstResponder(Some(renderer));
                        }
                    }
                });
            })
            .detach();
    }

    pub(super) fn synchronize(
        &mut self,
        spec: &Spec,
        bounds: Bounds<Pixels>,
        enabled: bool,
        opacity: f32,
        leading: bool,
        window: &Window,
        cx: &mut App,
    ) {
        let Some((renderer, _)) = views(window) else {
            self.hide(window, cx);
            return;
        };
        let current_surface = surface(window, cx);
        let target_changed = !same_target(&self.token, window, cx);
        if target_changed
            || !self.token.visible.get()
            || self.token.bounds.get() != bounds
            || self.token.surface.get() != current_surface
            || self.token.enabled.get() != enabled
        {
            self.token
                .revision
                .set(self.token.revision.get().wrapping_add(1));
        }
        if target_changed
            || !self.token.visible.get()
            || !same_surface(
                self.token.surface.get(),
                current_surface,
                matches!(spec, Spec::Search { .. }),
            )
            || self.token.enabled.get() != enabled
        {
            self.token
                .editing_revision
                .set(self.token.editing_revision.get().wrapping_add(1));
        }
        let view_bounds = renderer.bounds();
        let x = view_bounds.origin.x + f64::from(f32::from(bounds.origin.x));
        let y = if renderer.isFlipped() {
            view_bounds.origin.y + f64::from(f32::from(bounds.origin.y))
        } else {
            view_bounds.origin.y + view_bounds.size.height - f64::from(f32::from(bounds.bottom()))
        };
        let size = NSSize::new(
            f64::from(f32::from(bounds.size.width)),
            f64::from(f32::from(bounds.size.height)),
        );
        let host =
            super::group::painting_group(bounds, window, cx).unwrap_or_else(|| self._group.clone());
        host.add(&self.root);
        self.root.setAlphaValue(if host.joined() {
            1.0
        } else {
            f64::from(opacity)
        });
        if let Some(gate) = &self.host {
            gate.set_interactive(enabled);
        }
        if let Content::Button(button) = &self.content {
            // Native primary and selected bezels provide their own contrasting
            // foreground; ordinary neighbors share the group's glass surface.
            let emphasized = matches!(
                spec,
                Spec::Button {
                    role: super::ButtonRole::Primary,
                    ..
                }
            ) || matches!(
                spec,
                Spec::Button {
                    selected: Some(true),
                    ..
                }
            );
            button.setBordered(!host.joined() || emphasized);
        }
        self.root
            .setFrame(host.rect(NSRect::new(NSPoint::new(x, y), size), renderer));
        let local = NSRect::new(NSPoint::new(0.0, 0.0), size);
        if let Some(glass) = self.glass.as_ref() {
            glass.setFrame(local);
            if let Some(wrapper) = &self.status_container {
                wrapper.setFrame(glass.bounds());
            }
            // Public AppKit curvature for this passive status pill; there is
            // no extra glass platter around stock switches or search fields.
            unsafe {
                let _: () = msg_send![glass, setCornerRadius: size.height / 2.0];
            }
        }
        let inset = match self.content {
            Content::Status(_) => 4.0,
            Content::Switch(_) => 3.0,
            _ => 0.0,
        };
        let measured = super::measure::size(spec, window, cx).unwrap_or(size);
        let native_height = match self.content {
            Content::Status(_) => (measured.height - 8.0).max(0.0),
            Content::Switch(_) => (measured.height - 6.0).max(0.0),
            _ => measured.height,
        }
        .min((size.height - 2.0 * inset).max(0.0));
        let native_width = if matches!(self.content, Content::Switch(_)) {
            (measured.width - 6.0).max(0.0).min(size.width)
        } else if matches!(self.content, Content::Button(_) | Content::Popup(_)) {
            measured.width.min(size.width)
        } else {
            (size.width - 2.0 * inset).max(0.0)
        };
        self.content.view().setFrame(NSRect::new(
            NSPoint::new(
                (size.width - native_width) / 2.0,
                (size.height - native_height) / 2.0,
            ),
            NSSize::new(native_width, native_height),
        ));
        self.content.control().setEnabled(enabled);
        match (spec, &self.content) {
            (
                Spec::Button {
                    label,
                    accessibility,
                    selected,
                    icon,
                    role,
                },
                Content::Button(button),
            ) => {
                button.setAlignment(if leading {
                    NSTextAlignment::Left
                } else {
                    NSTextAlignment::Center
                });
                if let Some(icon) = icon {
                    button.setImage(
                        super::measure::symbol_image(*icon, accessibility, *selected, window, cx)
                            .as_deref(),
                    );
                }
                button.setToolTip(Some(&NSString::from_str(accessibility)));
                objc2_app_kit::NSAccessibility::setAccessibilityLabel(
                    &**button,
                    Some(&NSString::from_str(accessibility)),
                );
                if let Some(selected) = selected {
                    button.setState(if *selected { 1 } else { 0 });
                }
                button.setHasDestructiveAction(matches!(role, super::ButtonRole::Destructive));
                button.setTintProminence(match role {
                    super::ButtonRole::Primary => NSTintProminence::Primary,
                    super::ButtonRole::Destructive => NSTintProminence::Secondary,
                    super::ButtonRole::Normal if *selected == Some(true) => {
                        NSTintProminence::Primary
                    }
                    super::ButtonRole::Normal | super::ButtonRole::Toolbar => {
                        NSTintProminence::Automatic
                    }
                });
                if button.title().to_string() != label.as_ref() {
                    button.setTitle(&NSString::from_str(label));
                }
            }
            (Spec::Search { input, .. }, Content::Search(field)) => {
                let input = input.read(cx);
                let value = input.text();
                let model_revision = input.native_model_revision();
                let decision = events::synchronize(
                    &value,
                    self.last_value.as_deref(),
                    self.token.accepted_native.borrow().as_deref(),
                    model_revision,
                    self.token.model_revision.get(),
                );
                if decision != Sync::Unchanged {
                    if decision == Sync::Replace {
                        self.token
                            .editing_revision
                            .set(self.token.editing_revision.get().wrapping_add(1));
                        if field.stringValue().to_string() != value {
                            let text = NSString::from_str(&value);
                            field.setStringValue(&text);
                            if let Some(editor) = field.currentEditor() {
                                editor.setString(&text);
                            }
                        }
                    }
                    self.last_value = Some(value);
                    self.token.accepted_native.take();
                }
                self.token.model_revision.set(model_revision);
            }
            (Spec::Popup { input, values, .. }, Content::Popup(popup)) => {
                let input = input.read(cx);
                let revision = input.native_model_revision();
                if revision != self.token.model_revision.get() {
                    self.token
                        .editing_revision
                        .set(self.token.editing_revision.get().wrapping_add(1));
                }
                let value = input.text();
                for (index, choice) in values.iter().enumerate() {
                    if let Some(item) = popup.itemAtIndex((index + 1) as isize) {
                        item.setState(if choice == &value { 1 } else { 0 });
                    }
                }
                popup.selectItemAtIndex(0);
                self.token.model_revision.set(revision);
            }

            (Spec::Switch { value, .. }, Content::Switch(switch)) => {
                switch.setState(if *value { 1 } else { 0 })
            }
            (
                Spec::Status {
                    label,
                    color,
                    semantic,
                },
                Content::Status(field),
            ) => {
                field.setStringValue(&NSString::from_str(label));
                let tint = semantic.then(|| {
                    NSColor::colorWithSRGBRed_green_blue_alpha(
                        f64::from(color.r),
                        f64::from(color.g),
                        f64::from(color.b),
                        f64::from(color.a * crate::components::style::STATE_TINT_ALPHA),
                    )
                });
                if let Some(glass) = &self.glass {
                    // SAFETY: The retained view is the runtime-gated public
                    // NSGlassEffectView; clear tint when the state is neutral.
                    unsafe {
                        let _: () = msg_send![glass, setTintColor: tint.as_deref()];
                    }
                }
                let foreground = if *semantic {
                    let rgb = crate::components::contrast::tinted_text(
                        [f64::from(color.r), f64::from(color.g), f64::from(color.b)],
                        f64::from(color.a * crate::components::style::STATE_TINT_ALPHA),
                        crate::components::style::SURFACE,
                    );
                    NSColor::colorWithSRGBRed_green_blue_alpha(
                        f64::from((rgb >> 16) & 255) / 255.0,
                        f64::from((rgb >> 8) & 255) / 255.0,
                        f64::from(rgb & 255) / 255.0,
                        1.0,
                    )
                } else {
                    NSColor::secondaryLabelColor()
                };
                field.setTextColor(Some(&foreground));
            }
            _ => {}
        }
        let was_enabled = self.token.enabled.replace(enabled);
        if was_enabled && !enabled {
            // The editor belongs to the previous context. Capture it before
            // rebinding the retained control to this frame's machine/modal.
            self.finish_editing();
        }
        self.token.surface.set(current_surface);
        self.token.target.replace(current_target(window, cx));
        self.token.bounds.set(bounds);
        self.token.visible.set(true);
        self.root.setHidden(false);
    }
}
impl Drop for Leaf {
    fn drop(&mut self) {
        self.token.visible.set(false);
        self.finish_editing();
        if let Content::Search(field) = &self.content {
            unsafe {
                field.setDelegate(None);
            }
        }
        // NSControl targets are weak; clear them before releasing our retained target.
        unsafe {
            self.content.control().setTarget(None);
            self.content.control().setAction(None);
        }
        self.root.removeFromSuperview();
    }
}
