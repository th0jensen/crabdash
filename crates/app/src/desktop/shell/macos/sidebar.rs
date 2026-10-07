//! AppKit owns sidebar typography, selection, materials, and scrolling.
use super::Command;
use crate::app::Crabdash;
use crate::features::machines::logos;
use machines::machine::{Machine, MachineKind};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, runtime::ProtocolObject, sel,
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBezelStyle, NSButton, NSControlTextEditingDelegate, NSEvent,
    NSImage, NSImageScaling, NSImageView, NSLayoutConstraint, NSMenu, NSMenuItem, NSScrollView,
    NSTableCellView, NSTableColumn, NSTableView, NSTableViewColumnAutoresizingStyle,
    NSTableViewDataSource, NSTableViewDelegate, NSTableViewStyle, NSTextField,
    NSUserInterfaceItemIdentification, NSView, NSViewController,
};
use objc2_foundation::{
    NSArray, NSData, NSIndexSet, NSInteger, NSNotification, NSObject, NSObjectProtocol, NSRect,
    NSSize, NSString,
};
use smol::channel::Sender;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Artwork {
    Svg(&'static [u8]),
    Symbol(&'static str),
}

#[derive(Clone, Eq, PartialEq)]
struct Row {
    uuid: Uuid,
    name: String,
    tooltip: String,
    remote: bool,
    artwork: Artwork,
}

impl Row {
    fn from_machine(machine: &Machine) -> Self {
        let name = machine.system_info.machine_name.trim();
        let name = if name.is_empty() { &machine.id } else { name };
        let endpoint = machine.remote.as_ref().map_or_else(
            || "This machine".to_owned(),
            |remote| format!("{}@{}", remote.user, remote.host),
        );
        let status = if machine.connected() {
            "Connected"
        } else {
            "Disconnected"
        };
        let release = machine
            .system_info
            .distribution
            .as_ref()
            .map(|distribution| format!("{}\n", distribution.pretty_name))
            .unwrap_or_default();
        Self {
            uuid: machine.uuid,
            name: name.to_owned(),
            tooltip: format!(
                "{name} · {status}\n{endpoint}\n{release}{}",
                machine.system_info.os_version.trim()
            ),
            remote: machine.remote.is_some(),
            artwork: logos::machine_svg_bytes(machine)
                .map(Artwork::Svg)
                .unwrap_or_else(|| {
                    Artwork::Symbol(match machine.kind {
                        MachineKind::Windows => "pc",
                        _ => "desktopcomputer",
                    })
                }),
        }
    }
}

struct TableState {
    commands: Sender<Command>,
    rows: RefCell<Vec<Row>>,
    images: RefCell<HashMap<Artwork, Option<Retained<NSImage>>>>,
    synchronizing: Cell<bool>,
    blocked: Cell<bool>,
    context_uuid: Cell<Option<Uuid>>,
}

define_class!(
    // SAFETY: NSTableView has no additional subclassing requirements. AppKit
    // callbacks and all ivar access stay on the main thread.
    #[unsafe(super = NSTableView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TableState]
    struct SidebarTable;

    unsafe impl NSObjectProtocol for SidebarTable {}
    unsafe impl NSControlTextEditingDelegate for SidebarTable {}
    unsafe impl NSTableViewDataSource for SidebarTable {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _: &NSTableView) -> NSInteger {
            self.ivars().rows.borrow().len() as NSInteger + 1
        }
    }
    unsafe impl NSTableViewDelegate for SidebarTable {
        #[unsafe(method(tableView:isGroupRow:))]
        fn group_row(&self, _: &NSTableView, row: NSInteger) -> bool { row == 0 }

        #[unsafe(method(tableView:shouldSelectRow:))]
        fn should_select(&self, _: &NSTableView, row: NSInteger) -> bool {
            !self.ivars().blocked.get() && self.row(row).is_some()
        }

        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn row_view(&self, table: &NSTableView, _: Option<&NSTableColumn>, index: NSInteger) -> Option<Retained<NSView>> {
            (|| {
            let row = self.row(index);
            if index != 0 && row.is_none() { return None; }
            let cell = table_cell(table, index == 0, self.mtm());
            // SAFETY: table_cell always installs this cell's text field.
            if let Some(text) = unsafe { cell.textField() } {
                text.setStringValue(&NSString::from_str(row.as_ref().map_or("Machines", |row| row.name.as_str())));
            }
            cell.setToolTip(row.as_ref().map(|row| NSString::from_str(&row.tooltip)).as_deref());
            // SAFETY: table_cell installs an image view for machine cells only.
            if let Some(image_view) = unsafe { cell.imageView() } {
                let image = row.as_ref().and_then(|row| self.image(row.artwork));
                image_view.setImage(image.as_deref());
            }
            Some(cell.into_super())
            })()
        }

        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_changed(&self, _: &NSNotification) {
            if self.ivars().synchronizing.get() || self.ivars().blocked.get() { return; }
            if let Some(row) = self.row(self.selectedRow()) {
                let _ = self.ivars().commands.try_send(Command::SelectMachine(row.uuid));
            }
        }
    }
    impl SidebarTable {
        #[unsafe(method_id(menuForEvent:))]
        fn context_menu(&self, event: &NSEvent) -> Option<Retained<NSMenu>> {
            (|| {
            self.ivars().context_uuid.set(None);
            if self.ivars().blocked.get() { return None; }
            let point = self.convertPoint_fromView(event.locationInWindow(), None);
            let row = self.row(self.rowAtPoint(point))?;
            self.ivars().context_uuid.set(Some(row.uuid));
            let menu = self.menu()?;
            if let Some(delete) = menu.itemAtIndex(1) { delete.setEnabled(row.remote); }
            Some(menu)
            })()
        }
        #[unsafe(method(addMachine:))]
        fn add_machine(&self, _: Option<&NSObject>) {
            if !self.ivars().blocked.get() {
                let _ = self.ivars().commands.try_send(Command::AddMachine);
            }
        }
        #[unsafe(method(refreshMachine:))]
        fn refresh_machine(&self, _: Option<&NSObject>) {
            if let Some(row) = self.context_row() {
                let _ = self.ivars().commands.try_send(Command::RefreshMachine(row.uuid));
            }
        }
        #[unsafe(method(deleteMachine:))]
        fn delete_machine(&self, _: Option<&NSObject>) {
            if let Some(row) = self.context_row() && row.remote {
                let _ = self.ivars().commands.try_send(Command::DeleteMachine(row.uuid));
            }
        }
    }
);

impl SidebarTable {
    fn new(commands: Sender<Command>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TableState {
            commands,
            rows: RefCell::new(Vec::new()),
            images: RefCell::new(HashMap::new()),
            synchronizing: Cell::new(false),
            blocked: Cell::new(false),
            context_uuid: Cell::new(None),
        });
        // SAFETY: NSTableView's designated initializer initializes our subclass.
        unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] }
    }

    fn row(&self, index: NSInteger) -> Option<Row> {
        usize::try_from(index)
            .ok()?
            .checked_sub(1)
            .and_then(|index| self.ivars().rows.borrow().get(index).cloned())
    }

    fn context_row(&self) -> Option<Row> {
        if self.ivars().blocked.get() {
            return None;
        }
        let uuid = self.ivars().context_uuid.get()?;
        self.ivars()
            .rows
            .borrow()
            .iter()
            .find(|row| row.uuid == uuid)
            .cloned()
    }

    fn image(&self, artwork: Artwork) -> Option<Retained<NSImage>> {
        if let Some(image) = self.ivars().images.borrow().get(&artwork) {
            return image.clone();
        }
        let image = match artwork {
            Artwork::Svg(bytes) => template_svg(bytes),
            Artwork::Symbol(symbol) => {
                let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    &NSString::from_str(symbol),
                    None,
                );
                if let Some(image) = &image {
                    image.setTemplate(true);
                }
                image
            }
        };
        self.ivars()
            .images
            .borrow_mut()
            .insert(artwork, image.clone());
        image
    }
}

/// The table retains no GPUI entity or mutable machine references. Native
/// callbacks send UUID commands; the shared shell validates current app state.
pub(super) struct Sidebar {
    controller: Retained<NSViewController>,
    table: Retained<SidebarTable>,
    add: Retained<NSButton>,
}

impl Sidebar {
    pub(super) fn new(commands: Sender<Command>, mtm: MainThreadMarker) -> Self {
        let controller = NSViewController::new(mtm);
        let view = NSView::new(mtm);
        controller.setView(&view);

        let table = SidebarTable::new(commands, mtm);
        table.setStyle(NSTableViewStyle::SourceList);
        table.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        table.setHeaderView(None);
        table.setAllowsMultipleSelection(false);
        table.setAllowsColumnSelection(false);
        table.setAllowsColumnReordering(false);
        table.setColumnAutoresizingStyle(
            NSTableViewColumnAutoresizingStyle::LastColumnOnlyAutoresizingStyle,
        );
        let column = NSTableColumn::initWithIdentifier(
            NSTableColumn::alloc(mtm),
            &NSString::from_str("machines"),
        );
        table.addTableColumn(&column);
        // SAFETY: Sidebar owns the table throughout its weak data source,
        // delegate, button-target and menu-target lifetimes.
        unsafe {
            table.setDataSource(Some(ProtocolObject::from_ref(&*table)));
            table.setDelegate(Some(ProtocolObject::from_ref(&*table)));
        }
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Machine"));
        menu.setAutoenablesItems(false);
        for (title, action) in [
            ("Refresh", sel!(refreshMachine:)),
            ("Delete", sel!(deleteMachine:)),
        ] {
            // SAFETY: These actions are implemented above; the table outlives
            // the menu's weak target. Empty key equivalents avoid global shortcuts.
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::new(),
                )
            };
            unsafe {
                item.setTarget(Some(&table));
            }
            menu.addItem(&item);
        }
        unsafe {
            table.setMenu(Some(&menu));
        }

        let scroll = NSScrollView::new(mtm);
        scroll.setDrawsBackground(false);
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDocumentView(Some(&table));
        scroll.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.addSubview(&scroll);

        let add = NSButton::new(mtm);
        add.setTitle(&NSString::from_str("Add Machine…"));
        add.setBezelStyle(NSBezelStyle::Push);
        add.setTranslatesAutoresizingMaskIntoConstraints(false);
        // SAFETY: The retained table implements addMachine: and outlives the button.
        unsafe {
            add.setTarget(Some(&table));
            add.setAction(Some(sel!(addMachine:)));
        }
        view.addSubview(&add);
        let safe_area = view.safeAreaLayoutGuide();
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
            scroll
                .leadingAnchor()
                .constraintEqualToAnchor(&safe_area.leadingAnchor()),
            scroll
                .trailingAnchor()
                .constraintEqualToAnchor(&safe_area.trailingAnchor()),
            scroll
                .topAnchor()
                .constraintEqualToAnchor(&safe_area.topAnchor()),
            scroll
                .bottomAnchor()
                .constraintEqualToAnchor_constant(&add.topAnchor(), -8.0),
            add.leadingAnchor()
                .constraintEqualToAnchor_constant(&safe_area.leadingAnchor(), 12.0),
            add.trailingAnchor()
                .constraintEqualToAnchor_constant(&safe_area.trailingAnchor(), -12.0),
            add.bottomAnchor()
                .constraintEqualToAnchor_constant(&safe_area.bottomAnchor(), -12.0),
        ]));
        Self {
            controller,
            table,
            add,
        }
    }

    pub(super) fn controller(&self) -> &NSViewController {
        &self.controller
    }

    pub(super) fn synchronize(&self, app: &Crabdash) {
        let blocked = super::commands_blocked(app) || app.workspaces.open;
        self.table.ivars().blocked.set(blocked);
        self.table.setEnabled(!blocked);
        self.add.setEnabled(!blocked);
        if blocked {
            self.table.ivars().context_uuid.set(None);
        }
        let rows: Vec<_> = app
            .machine_store
            .machines
            .iter()
            .map(Row::from_machine)
            .collect();
        let changed = *self.table.ivars().rows.borrow() != rows;
        let selected = app
            .machine_store
            .machines
            .get(app.selected_machine)
            .map(|machine| machine.uuid);
        self.table.ivars().synchronizing.set(true);
        if changed {
            *self.table.ivars().rows.borrow_mut() = rows;
            self.table.reloadData();
        }
        let index = selected.and_then(|uuid| {
            self.table
                .ivars()
                .rows
                .borrow()
                .iter()
                .position(|row| row.uuid == uuid)
        });
        if let Some(index) = index
            && self.table.selectedRow() != (index + 1) as NSInteger
        {
            self.table.selectRowIndexes_byExtendingSelection(
                &NSIndexSet::indexSetWithIndex(index + 1),
                false,
            );
        }
        self.table.ivars().synchronizing.set(false);
    }
}

fn table_cell(
    table: &NSTableView,
    group: bool,
    mtm: MainThreadMarker,
) -> Retained<NSTableCellView> {
    let identifier = NSString::from_str(if group { "group" } else { "machine" });
    // SAFETY: These identifiers are assigned only to NSTableCellView instances
    // created here; no nib or other cell class is registered with this table.
    if let Some(view) = unsafe { table.makeViewWithIdentifier_owner(&identifier, None) } {
        return unsafe { Retained::cast_unchecked(view) };
    }
    let cell = NSTableCellView::new(mtm);
    cell.setIdentifier(Some(&identifier));
    let text = NSTextField::labelWithString(&NSString::new(), mtm);
    text.setTranslatesAutoresizingMaskIntoConstraints(false);
    cell.addSubview(&text);
    // SAFETY: The cell retains the text field as a subview.
    unsafe {
        cell.setTextField(Some(&text));
    }
    let mut constraints = vec![
        text.centerYAnchor()
            .constraintEqualToAnchor(&cell.centerYAnchor()),
        text.trailingAnchor()
            .constraintEqualToAnchor_constant(&cell.trailingAnchor(), -4.0),
    ];
    if group {
        constraints.push(
            text.leadingAnchor()
                .constraintEqualToAnchor_constant(&cell.leadingAnchor(), 4.0),
        );
    } else {
        let image = NSImageView::new(mtm);
        image.setTranslatesAutoresizingMaskIntoConstraints(false);
        image.setImageScaling(NSImageScaling::ScaleProportionallyDown);
        cell.addSubview(&image);
        // SAFETY: The cell retains the image view as a subview.
        unsafe {
            cell.setImageView(Some(&image));
        }
        constraints.extend([
            image
                .leadingAnchor()
                .constraintEqualToAnchor_constant(&cell.leadingAnchor(), 4.0),
            image
                .centerYAnchor()
                .constraintEqualToAnchor(&cell.centerYAnchor()),
            image.widthAnchor().constraintEqualToConstant(16.0),
            image.heightAnchor().constraintEqualToConstant(16.0),
            text.leadingAnchor()
                .constraintEqualToAnchor_constant(&image.trailingAnchor(), 6.0),
        ]);
    }
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));
    cell
}

fn template_svg(bytes: &[u8]) -> Option<Retained<NSImage>> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(32, 32)?;
    let scale = (32.0 / size.width()).min(32.0 / size.height());
    let transform = resvg::tiny_skia::Transform::from_row(
        scale,
        0.0,
        0.0,
        scale,
        (32.0 - size.width() * scale) / 2.0,
        (32.0 - size.height() * scale) / 2.0,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let png = pixmap.encode_png().ok()?;
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&png))?;
    image.setSize(NSSize::new(16.0, 16.0));
    image.setTemplate(true);
    Some(image)
}
