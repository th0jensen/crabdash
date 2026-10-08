//! Public AppKit controls; no renderer is placed behind a sibling glass backdrop.
use super::{Event, Spec, leaf::Token};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{
    NSAccessibility, NSBezelStyle, NSButton, NSButtonType, NSCellImagePosition, NSControl,
    NSControlTextEditingDelegate, NSEvent, NSImage, NSLineBreakMode, NSPopUpButton, NSSearchField,
    NSSearchFieldDelegate, NSSwitch, NSTextAlignment, NSTextField, NSTextFieldDelegate, NSView,
};
use objc2_foundation::{NSNotification, NSObject, NSObjectProtocol, NSString};
use smol::channel::Sender;
use std::{cell::Cell, rc::Rc};
pub(super) struct TargetState {
    sender: Sender<Event>,
    token: Rc<Token>,
    choices: Option<Vec<String>>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TargetState]
    pub(super) struct Target;
    unsafe impl NSObjectProtocol for Target {}
    unsafe impl NSTextFieldDelegate for Target {}
    unsafe impl NSSearchFieldDelegate for Target {}
    unsafe impl NSControlTextEditingDelegate for Target {
        #[unsafe(method(controlTextDidBeginEditing:))]
        fn begin(&self, _: &NSNotification) {
            self.queue_focus();
        }
    }
    impl Target {
        #[unsafe(method(crabdashControl:))]
        fn activate(&self, sender: &NSControl) {
            if let Some(values) = self.ivars().choices.as_ref() {
                if let Some(popup) = sender.downcast_ref::<NSPopUpButton>() {
                    // The first pull-down item is the chooser header.
                    if let Ok(index) = usize::try_from(popup.indexOfSelectedItem() - 1) {
                        if let Some(value) = values.get(index) {
                            self.queue_action(value.clone());
                        }
                    }
                }
            } else {
                self.queue_action(sender.stringValue().to_string());
            }
        }
    }
);
impl Target {
    fn queue_focus(&self) {
        let state = self.ivars();
        let _ = state.sender.try_send(Event::Focus {
            stamp: state.token.stamp(),
        });
    }
    pub(super) fn queue_action(&self, text: String) {
        let state = self.ivars();
        state
            .token
            .sequence
            .set(state.token.sequence.get().wrapping_add(1));
        let _ = state.sender.try_send(Event::Action {
            stamp: state.token.stamp(),
            text,
        });
    }
    pub(super) fn new(
        sender: Sender<Event>,
        token: Rc<Token>,
        choices: Option<Vec<String>>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let allocated = Self::alloc(mtm).set_ivars(TargetState {
            sender,
            token,
            choices,
        });
        // SAFETY: NSObject initialization of a retained main-thread target.
        unsafe { msg_send![super(allocated), init] }
    }
}

define_class!(
    #[unsafe(super = NSSearchField)]
    #[thread_kind = MainThreadOnly]
    struct SearchField;
    unsafe impl NSObjectProtocol for SearchField {}
    impl SearchField {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            // Reusing AppKit's field editor need not start a new editing
            // session. Mirror the actual click without changing its responder.
            self.queue_focus();
            unsafe { let _: () = msg_send![super(self), mouseDown: event]; }
        }
        #[unsafe(method(becomeFirstResponder))]
        fn become_first_responder(&self) -> bool {
            let accepted: bool = unsafe { msg_send![super(self), becomeFirstResponder] };
            if accepted { self.queue_focus(); }
            accepted
        }
    }
);
impl SearchField {
    fn queue_focus(&self) {
        // The NSControl target is weak; Leaf retains it and clears it before
        // teardown. Retain the returned target for this short dispatch.
        if let Some(target) = self.target() {
            if let Some(target) = target.downcast_ref::<Target>() {
                target.queue_focus();
            }
        }
    }
}
// Passive pills never intercept the row's selection/context-menu interaction.
define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    pub(super) struct Passive;
    unsafe impl NSObjectProtocol for Passive {}
    impl Passive {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _: objc2_foundation::NSPoint) -> Option<Retained<NSView>> { None }
    }
);

// Disabled material stays visible, but must never intercept the renderer's
// modal or drag input. Empty host regions are also pass-through.
define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Cell<bool>]
    pub(super) struct Host;
    unsafe impl NSObjectProtocol for Host {}
    impl Host {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: objc2_foundation::NSPoint) -> Option<Retained<NSView>> {
            if self.ivars().get() {
                // SAFETY: Standard NSView hit testing on its retained main-thread subtree.
                let hit: Option<Retained<NSView>> = unsafe { msg_send![super(self), hitTest: point] };
                hit.filter(|view| Retained::as_ptr(view).cast::<()>() != (self as *const Self).cast())
            } else {
                None
            }
        }
    }
);
impl Host {
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: Initialize this NSView subclass after its interaction gate.
        unsafe { msg_send![super(Self::alloc(mtm).set_ivars(Cell::new(false))), init] }
    }
    pub(super) fn set_interactive(&self, interactive: bool) {
        self.ivars().set(interactive);
    }
}

pub(super) enum Content {
    Button(Retained<NSButton>),
    Search(Retained<NSSearchField>),
    Popup(Retained<NSPopUpButton>),
    Switch(Retained<NSSwitch>),
    Status(Retained<NSTextField>),
}
impl Content {
    pub(super) fn view(&self) -> &NSView {
        match self {
            Self::Button(view) => view,
            Self::Search(view) => view,
            Self::Popup(view) => view,
            Self::Switch(view) => view,
            Self::Status(view) => view,
        }
    }
    pub(super) fn control(&self) -> &NSControl {
        match self {
            Self::Button(view) => view,
            Self::Search(view) => view,
            Self::Popup(view) => view,
            Self::Switch(view) => view,
            Self::Status(view) => view,
        }
    }
    pub(super) fn new(spec: &Spec, mtm: MainThreadMarker) -> Self {
        match spec {
            Spec::Button {
                label,
                accessibility,
                icon,
                selected,
                role,
            } => {
                let button = NSButton::new(mtm);
                button.setTitle(&NSString::from_str(label));
                button.setAlignment(NSTextAlignment::Center);
                if let Some(cell) = button.cell() {
                    cell.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
                }
                if selected.is_some() {
                    button.setButtonType(NSButtonType::PushOnPushOff);
                }
                button.setImagePosition(if label.is_empty() {
                    NSCellImagePosition::ImageOnly
                } else {
                    NSCellImagePosition::ImageLeading
                });
                button.setBezelStyle(if matches!(role, super::ButtonRole::Toolbar) {
                    NSBezelStyle::Toolbar
                } else {
                    NSBezelStyle::Glass
                });
                if let Some(icon) = icon {
                    if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                        &NSString::from_str(super::widgets::button_symbol(*icon, *selected)),
                        Some(&NSString::from_str(accessibility)),
                    ) {
                        button.setImage(Some(&image));
                    }
                }
                button.setToolTip(Some(&NSString::from_str(accessibility)));
                button.setAccessibilityLabel(Some(&NSString::from_str(accessibility)));
                Self::Button(button)
            }
            Spec::Search { placeholder, .. } => {
                let field: Retained<SearchField> =
                    unsafe { msg_send![super(SearchField::alloc(mtm).set_ivars(())), init] };
                field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
                field.setAccessibilityLabel(Some(&NSString::from_str(placeholder)));
                field.setToolTip(Some(&NSString::from_str(placeholder)));
                field.setSendsWholeSearchString(false);
                field.setSendsSearchStringImmediately(true);
                Self::Search(Retained::into_super(field))
            }
            Spec::Popup { values, help, .. } => {
                let popup = NSPopUpButton::initWithFrame_pullsDown(
                    NSPopUpButton::alloc(mtm),
                    objc2_foundation::NSRect::default(),
                    true,
                );
                popup.setBezelStyle(NSBezelStyle::Glass);
                popup.setAutoenablesItems(false);
                popup.setAltersStateOfSelectedItem(false);
                popup.addItemWithTitle(&NSString::from_str(" "));
                for value in values {
                    popup.addItemWithTitle(&NSString::from_str(if value.is_empty() {
                        "System font"
                    } else {
                        value
                    }));
                }
                popup.setToolTip(Some(&NSString::from_str(help)));
                popup.setAccessibilityLabel(Some(&NSString::from_str(help)));
                Self::Popup(popup)
            }
            Spec::Switch { label, .. } => {
                let switch = NSSwitch::new(mtm);
                switch.setToolTip(Some(&NSString::from_str(label)));
                switch.setAccessibilityLabel(Some(&NSString::from_str(label)));
                Self::Switch(switch)
            }
            Spec::Status { label, .. } => {
                let field = NSTextField::labelWithString(&NSString::from_str(label), mtm);
                field.setAlignment(NSTextAlignment::Center);
                field.setUsesSingleLineMode(true);
                Self::Status(field)
            }
        }
    }
}
