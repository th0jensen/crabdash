//! Stock AppKit toolbar items; actions are queued into the dashboard controller.
use super::Command;
use crate::app::Crabdash;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::Sel, sel,
};
use objc2_app_kit::{
    NSImage, NSToolbar, NSToolbarDelegate, NSToolbarDisplayMode,
    NSToolbarFlexibleSpaceItemIdentifier, NSToolbarItem, NSToolbarItemGroup,
    NSToolbarItemGroupSelectionMode, NSToolbarItemIdentifier,
    NSToolbarSidebarTrackingSeparatorItemIdentifier, NSToolbarSpaceItemIdentifier,
    NSToolbarToggleSidebarItemIdentifier,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString};
use smol::channel::Sender;
use std::cell::RefCell;

const REFRESH: &str = "CrabdashRefresh";
const GROUP: &str = "CrabdashTerminalWorkspaces";

define_class!(
    // SAFETY: NSObject has no subclassing requirements. AppKit's weak targets
    // are retained by the per-window toolbar delegate.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Sender<Command>]
    struct ToolbarTarget;
    unsafe impl NSObjectProtocol for ToolbarTarget {}
    impl ToolbarTarget {
        #[unsafe(method(crabdashRefresh:))]
        fn refresh(&self, _sender: Option<&NSObject>) {
            let _ = self.ivars().try_send(Command::Refresh);
        }
        #[unsafe(method(crabdashTerminal:))]
        fn terminal(&self, _sender: Option<&NSObject>) {
            let _ = self.ivars().try_send(Command::Terminal);
        }
        #[unsafe(method(crabdashWorkspaces:))]
        fn workspaces(&self, _sender: Option<&NSObject>) {
            let _ = self.ivars().try_send(Command::Workspaces);
        }
    }
);

#[derive(Clone, Default)]
struct Items {
    refresh: Option<Retained<NSToolbarItem>>,
    group: Option<Retained<NSToolbarItemGroup>>,
}
struct DelegateState {
    target: Retained<ToolbarTarget>,
    inserted: RefCell<Items>,
}

fn identifiers() -> Retained<NSArray<NSToolbarItemIdentifier>> {
    let refresh = NSString::from_str(REFRESH);
    let group = NSString::from_str(GROUP);
    // SAFETY: These are immutable AppKit identifiers available on supported
    // macOS versions. AppKit creates the sidebar toggle and tracking separator.
    unsafe {
        NSArray::from_slice(&[
            NSToolbarToggleSidebarItemIdentifier,
            NSToolbarSidebarTrackingSeparatorItemIdentifier,
            NSToolbarFlexibleSpaceItemIdentifier,
            &refresh,
            NSToolbarSpaceItemIdentifier,
            &group,
        ])
    }
}

fn item(
    identifier: &str,
    label: &str,
    symbol: &str,
    action: Sel,
    target: &ToolbarTarget,
    mtm: MainThreadMarker,
) -> Retained<NSToolbarItem> {
    let item = NSToolbarItem::initWithItemIdentifier(
        NSToolbarItem::alloc(mtm),
        &NSString::from_str(identifier),
    );
    let label = NSString::from_str(label);
    item.setLabel(&label);
    item.setPaletteLabel(&label);
    item.setImage(
        NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            Some(&label),
        )
        .as_deref(),
    );
    // AppKit supplies the material, metrics, symbol sizing and appearance.
    item.setBordered(true);
    item.setAutovalidates(false);
    // SAFETY: The retained target implements each selector passed here.
    unsafe {
        item.setTarget(Some(target));
        item.setAction(Some(action));
    }
    item
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; each delegate belongs
    // to one toolbar and creates fresh items for every AppKit request.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateState]
    struct ToolbarDelegate;
    unsafe impl NSObjectProtocol for ToolbarDelegate {}
    unsafe impl NSToolbarDelegate for ToolbarDelegate {
        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn defaults(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSToolbarItemIdentifier>> {
            identifiers()
        }
        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn allowed(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSToolbarItemIdentifier>> {
            identifiers()
        }
        #[unsafe(method_id(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:))]
        fn make_item(
            &self,
            _toolbar: &NSToolbar,
            identifier: &NSToolbarItemIdentifier,
            inserted: bool,
        ) -> Option<Retained<NSToolbarItem>> {
            let mtm = self.mtm();
            let target = &self.ivars().target;
            match identifier.to_string().as_str() {
                REFRESH => {
                    let refresh = item(
                        REFRESH,
                        "Refresh",
                        "arrow.clockwise",
                        sel!(crabdashRefresh:),
                        target,
                        mtm,
                    );
                    refresh.setToolTip(Some(&NSString::from_str("Refresh · ⌘R")));
                    if inserted {
                        self.ivars().inserted.borrow_mut().refresh = Some(refresh.clone());
                    }
                    Some(refresh)
                }
                GROUP => {
                    let terminal = item(
                        "CrabdashTerminal",
                        "Terminal",
                        "terminal",
                        sel!(crabdashTerminal:),
                        target,
                        mtm,
                    );
                    let workspaces = item(
                        "CrabdashWorkspaces",
                        "Workspaces",
                        "rectangle.split.2x2",
                        sel!(crabdashWorkspaces:),
                        target,
                        mtm,
                    );
                    let group = NSToolbarItemGroup::initWithItemIdentifier(
                        NSToolbarItemGroup::alloc(mtm),
                        identifier,
                    );
                    let label = NSString::from_str("Terminal and Workspaces");
                    group.setLabel(&label);
                    group.setPaletteLabel(&label);
                    group.setBordered(true);
                    group.setAutovalidates(false);
                    group.setSelectionMode(NSToolbarItemGroupSelectionMode::SelectAny);
                    group.setSubitems(&NSArray::from_slice(&[&*terminal, &*workspaces]));
                    if inserted {
                        self.ivars().inserted.borrow_mut().group = Some(group.clone());
                    }
                    Some(group.into_super())
                }
                // Built-in items are manufactured and validated by AppKit.
                _ => None,
            }
        }
    }
);

pub(super) struct Toolbar {
    toolbar: Retained<NSToolbar>,
    delegate: Retained<ToolbarDelegate>,
}
impl Toolbar {
    pub(super) fn new(commands: Sender<Command>, mtm: MainThreadMarker) -> Self {
        let target = ToolbarTarget::alloc(mtm).set_ivars(commands);
        // SAFETY: NSObject initializes the allocated action target.
        let target: Retained<ToolbarTarget> = unsafe { msg_send![super(target), init] };
        let delegate = ToolbarDelegate::alloc(mtm).set_ivars(DelegateState {
            target,
            inserted: RefCell::new(Items::default()),
        });
        // SAFETY: NSObject initializes the allocated toolbar delegate.
        let delegate: Retained<ToolbarDelegate> = unsafe { msg_send![super(delegate), init] };
        let toolbar = NSToolbar::initWithIdentifier(
            NSToolbar::alloc(mtm),
            &NSString::from_str("CrabdashToolbar"),
        );
        toolbar.setDisplayMode(NSToolbarDisplayMode::IconOnly);
        toolbar.setAllowsUserCustomization(false);
        toolbar.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*delegate)));
        Self { toolbar, delegate }
    }

    pub(super) fn toolbar(&self) -> &NSToolbar {
        &self.toolbar
    }

    pub(super) fn synchronize(&self, app: &Crabdash) {
        let enabled = !super::commands_blocked(app);
        for item in self.toolbar.items() {
            // Keep the system-provided sidebar toggle under the same modal
            // policy as the dashboard actions, without changing its stock view.
            if item.itemIdentifier().to_string()
                == unsafe { NSToolbarToggleSidebarItemIdentifier }.to_string()
            {
                item.setAutovalidates(false);
                let tooltip = if app.sidebar_collapsed {
                    "Show Sidebar"
                } else {
                    "Hide Sidebar"
                };
                if item
                    .toolTip()
                    .is_none_or(|value| value.to_string() != tooltip)
                {
                    item.setToolTip(Some(&NSString::from_str(tooltip)));
                }
                if item.isEnabled() != enabled {
                    item.setEnabled(enabled);
                }
            }
        }
        // No RefCell borrow crosses AppKit work that could call its delegate.
        let items = self.delegate.ivars().inserted.borrow().clone();
        if let Some(refresh) = &items.refresh {
            if refresh.isEnabled() != enabled {
                refresh.setEnabled(enabled);
            }
        }
        if let Some(group) = &items.group {
            if group.isEnabled() != enabled {
                group.setEnabled(enabled);
            }
            for (index, selected) in [app.quake_terminal_open, app.workspaces.open]
                .into_iter()
                .enumerate()
            {
                if group.isSelectedAtIndex(index as isize) != selected {
                    group.setSelected_atIndex(selected, index as isize);
                }
            }
            for (index, item) in group.subitems().iter().enumerate() {
                if item.isEnabled() != enabled {
                    item.setEnabled(enabled);
                }
                let (selected, tooltip) = if index == 0 {
                    (
                        app.quake_terminal_open,
                        if app.quake_terminal_open {
                            "Hide terminal · ⌘J"
                        } else {
                            "Show terminal · ⌘J"
                        },
                    )
                } else {
                    (
                        app.workspaces.open,
                        if app.workspaces.open {
                            "Close workspaces"
                        } else {
                            "Workspaces"
                        },
                    )
                };
                if item
                    .toolTip()
                    .is_none_or(|value| value.to_string() != tooltip)
                {
                    item.setToolTip(Some(&NSString::from_str(tooltip)));
                }
                if let Some(menu) = item.menuFormRepresentation() {
                    let state = if selected { 1 } else { 0 };
                    if menu.state() != state {
                        menu.setState(state);
                    }
                }
            }
        }
    }
}
impl Drop for Toolbar {
    fn drop(&mut self) {
        self.toolbar.setDelegate(None);
        let items = self.delegate.ivars().inserted.borrow().clone();
        // SAFETY: Disconnect AppKit's weak action targets before their owner drops.
        unsafe {
            if let Some(refresh) = &items.refresh {
                refresh.setTarget(None);
                refresh.setAction(None);
            }
            if let Some(group) = &items.group {
                for item in group.subitems() {
                    item.setTarget(None);
                    item.setAction(None);
                }
            }
        }
    }
}
