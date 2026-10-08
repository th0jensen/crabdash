//! Native sidebar navigation on the dashboard’s opaque surface.
use super::Command;
use crate::app::Crabdash;
use crate::features::machines::{logos, sidebar::palette};
use machines::machine::{Machine, MachineKind};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, runtime::ProtocolObject, sel,
};
use objc2_app_kit::{
    NSAppearanceCustomization, NSAutoresizingMaskOptions, NSBackgroundStyle, NSBezelStyle,
    NSBezierPath, NSButton, NSCellImagePosition, NSColor, NSColorSpace,
    NSControlTextEditingDelegate, NSEvent, NSFont, NSFontWeightMedium, NSImage, NSImageScaling,
    NSImageView, NSLayoutConstraint, NSLineBreakMode, NSMenu, NSMenuItem, NSScrollView,
    NSTableCellView, NSTableColumn, NSTableView, NSTableViewColumnAutoresizingStyle,
    NSTableViewDataSource, NSTableViewDelegate, NSTableViewStyle, NSTextField,
    NSUserInterfaceItemIdentification, NSView, NSViewController,
};
use objc2_foundation::{
    NSArray, NSData, NSIndexSet, NSInteger, NSNotification, NSObject, NSObjectProtocol,
    NSOperatingSystemVersion, NSPoint, NSProcessInfo, NSRect, NSSize, NSString,
};
use smol::channel::Sender;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Artwork {
    Svg(&'static [u8], u32),
    Symbol(&'static str),
}

#[derive(Clone, Eq, PartialEq)]
struct Row {
    uuid: Uuid,
    name: String,
    metadata: String,
    connected: bool,
    tooltip: String,
    remote: bool,
    artwork: Artwork,
}

impl Row {
    fn from_machine(machine: &Machine) -> Self {
        let name = machine.display_name();
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
            metadata: format!(
                "{} · {}",
                logos::platform_label(machine),
                if machine.remote.is_some() {
                    "SSH"
                } else {
                    "Local"
                }
            ),
            connected: machine.connected(),
            tooltip: format!(
                "{name} · {status}\n{endpoint}\n{release}{}",
                machine.system_info.os_version.trim()
            ),
            remote: machine.remote.is_some(),
            artwork: logos::machine_svg_artwork(machine)
                .map(|(bytes, color)| Artwork::Svg(bytes, color))
                .unwrap_or_else(|| {
                    Artwork::Symbol(match machine.kind {
                        MachineKind::Windows => "pc",
                        _ => "desktopcomputer",
                    })
                }),
        }
    }
}

struct CellLabels {
    selected: Cell<bool>,
    artwork_tint: Cell<Option<u32>>,
    metadata: Retained<NSTextField>,
    status: Retained<NSImageView>,
}

define_class!(
    // SAFETY: NSTableCellView has no additional subclassing requirements;
    // AppKit owns the native row selection and supplies backgroundStyle changes.
    #[unsafe(super = NSTableCellView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = CellLabels]
    struct MachineCell;
    unsafe impl NSObjectProtocol for MachineCell {}
    impl MachineCell {
        #[unsafe(method(setBackgroundStyle:))]
        fn background_style(&self, style: NSBackgroundStyle) {
            // SAFETY: Forward AppKit's style before adapting selected foregrounds.
            unsafe { let _: () = msg_send![super(self), setBackgroundStyle: style]; }
            self.refresh_colors();
        }
    }
);

impl MachineCell {
    fn refresh_colors(&self) {
        let selected =
            self.ivars().selected.get() || self.backgroundStyle() == NSBackgroundStyle::Emphasized;
        let primary = if selected {
            selection_text_color(
                self,
                self.backgroundStyle() == NSBackgroundStyle::Emphasized,
            )
        } else {
            NSColor::labelColor()
        };
        // SAFETY: machine_cell installs the primary field and image view.
        if let Some(text) = unsafe { self.textField() } {
            text.setTextColor(Some(&primary));
        }
        let metadata = if selected {
            primary.clone()
        } else {
            NSColor::secondaryLabelColor()
        };
        self.ivars().metadata.setTextColor(Some(&metadata));
        if let Some(image) = unsafe { self.imageView() } {
            let tint = if selected {
                primary
            } else {
                self.ivars()
                    .artwork_tint
                    .get()
                    .map_or_else(NSColor::labelColor, native_color)
            };
            image.setContentTintColor(Some(&tint));
        }
    }
}

fn native_color(rgb: u32) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from((rgb >> 16) & 255) / 255.0,
        f64::from((rgb >> 8) & 255) / 255.0,
        f64::from(rgb & 255) / 255.0,
        1.0,
    )
}

fn selection_text_color(view: &NSView, emphasized: bool) -> Retained<NSColor> {
    let result = RefCell::new(if emphasized {
        NSColor::selectedControlTextColor()
    } else {
        NSColor::labelColor()
    });
    let resolve = block2::StackBlock::new(|| {
        let background = if emphasized {
            NSColor::selectedContentBackgroundColor()
        } else {
            NSColor::unemphasizedSelectedContentBackgroundColor()
        };
        let Some(background) = background.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
        else {
            return;
        };
        let components = [
            background.redComponent(),
            background.greenComponent(),
            background.blueComponent(),
        ];
        let alpha = background.alphaComponent();
        if !alpha.is_finite() || !components.iter().all(|component| component.is_finite()) {
            return;
        }
        let surface = crate::components::style::CONTENT;
        let surface =
            [surface >> 16, surface >> 8, surface].map(|channel| f64::from(channel & 255) / 255.0);
        let alpha = alpha.clamp(0.0, 1.0);
        let components = std::array::from_fn(|channel| {
            components[channel].clamp(0.0, 1.0) * alpha + surface[channel] * (1.0 - alpha)
        });
        *result.borrow_mut() = native_color(palette::selection_text(components));
    });
    view.effectiveAppearance()
        .performAsCurrentDrawingAppearance(&resolve);
    result.into_inner()
}

define_class!(
    // SAFETY: This ordinary NSView paints its full bounds with an opaque colour.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct SidebarSurface;
    unsafe impl NSObjectProtocol for SidebarSurface {}
    impl SidebarSurface {
        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool { true }

        #[unsafe(method(drawRect:))]
        fn draw_surface(&self, _: NSRect) {
            native_color(crate::components::style::CONTENT).setFill();
            NSBezierPath::fillRect(self.bounds());
            // The safe area starts below the toolbar. Keep its divider inside
            // this surface, with no outline along the toolbar's left edge.
            let content = self.safeAreaLayoutGuide().frame();
            native_color(crate::components::style::BORDER).setFill();
            NSBezierPath::fillRect(NSRect::new(
                NSPoint::new(self.bounds().origin.x, content.origin.y + content.size.height - 1.0),
                NSSize::new(self.bounds().size.width, 1.0),
            ));
        }
    }
);

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

        #[unsafe(method(tableView:heightOfRow:))]
        fn row_height(&self, _: &NSTableView, index: NSInteger) -> f64 {
            if index == 0 { 36.0 } else { 60.0 }
        }

        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn row_view(&self, table: &NSTableView, _: Option<&NSTableColumn>, index: NSInteger) -> Option<Retained<NSView>> {
            (|| {
                if index == 0 {
                    let cell = group_cell(table, self.mtm());
                    // SAFETY: group_cell retains its primary label as a subview.
                    if let Some(text) = unsafe { cell.textField() } {
                        text.setStringValue(&NSString::from_str(&format!("Machines · {}", self.ivars().rows.borrow().len())));
                    }
                    return Some(cell.into_super());
                }
                let row = self.row(index)?;
                let cell = machine_cell(table, self.mtm());
                // SAFETY: machine_cell installs and retains these standard cell fields.
                if let Some(text) = unsafe { cell.textField() } {
                    text.setStringValue(&NSString::from_str(&row.name));
                }
                cell.ivars().metadata.setStringValue(&NSString::from_str(&row.metadata));
                cell.setToolTip(Some(&NSString::from_str(&row.tooltip)));
                if let Some(image_view) = unsafe { cell.imageView() } {
                    image_view.setImage(self.image(row.artwork).as_deref());
                }
                cell.ivars().artwork_tint.set(match row.artwork {
                    Artwork::Svg(_, color) => Some(color),
                    Artwork::Symbol(_) => None,
                });
                cell.ivars().selected.set(table.isRowSelected(index));
                let status = if row.connected { NSColor::systemGreenColor() } else { NSColor::secondaryLabelColor() };
                cell.ivars().status.setContentTintColor(Some(&status));
                cell.ivars().status.setToolTip(Some(&NSString::from_str(if row.connected { "Connected" } else { "Disconnected" })));
                cell.refresh_colors();
                Some(cell.into_super().into_super())
            })()
        }

        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_changed(&self, _: &NSNotification) {
            self.refresh_cells();
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
            if let Some(delete) = menu.itemAtIndex(2) { delete.setEnabled(row.remote); }
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
        #[unsafe(method(renameMachine:))]
        fn rename_machine(&self, _: Option<&NSObject>) {
            if let Some(row) = self.context_row() {
                let _ = self.ivars().commands.try_send(Command::RenameMachine(row.uuid));
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

    fn refresh_cells(&self) {
        let count = self.ivars().rows.borrow().len();
        for index in 1..=count {
            let row = index as NSInteger;
            if let Some(view) = self.viewAtColumn_row_makeIfNecessary(0, row, false)
                && let Ok(cell) = view.downcast::<MachineCell>()
            {
                cell.ivars().selected.set(self.isRowSelected(row));
                cell.refresh_colors();
            }
        }
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
            Artwork::Svg(bytes, _) => template_svg(bytes),
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
        let allocated = SidebarSurface::alloc(mtm).set_ivars(());
        // SAFETY: NSView's designated initializer initializes the opaque container.
        let view: Retained<SidebarSurface> =
            unsafe { msg_send![super(allocated), initWithFrame: NSRect::ZERO] };
        view.setWantsLayer(true);
        view.setClipsToBounds(true);
        controller.setView(&view);

        let table = SidebarTable::new(commands, mtm);
        table.setStyle(NSTableViewStyle::SourceList);
        // Set after SourceList: AppKit then uses its normal native selection
        // highlight, rather than the source-list material and blurred highlight.
        table.setBackgroundColor(&native_color(crate::components::style::CONTENT));
        table.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        table.setHeaderView(None);
        table.setIntercellSpacing(NSSize::new(0.0, 6.0));
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
            ("Rename…", sel!(renameMachine:)),
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
        scroll.setDrawsBackground(true);
        scroll.setBackgroundColor(&native_color(crate::components::style::CONTENT));
        scroll.setHasVerticalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setDocumentView(Some(&table));
        scroll.setTranslatesAutoresizingMaskIntoConstraints(false);
        view.addSubview(&scroll);

        let add = NSButton::new(mtm);
        add.setTitle(&NSString::from_str("Add machine"));
        add.setImage(
            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str("plus"),
                None,
            )
            .as_deref(),
        );
        add.setImagePosition(NSCellImagePosition::ImageLeft);
        add.setToolTip(Some(&NSString::from_str("Add machine · ⌘N")));
        let glass = NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(
            NSOperatingSystemVersion {
                majorVersion: 26,
                minorVersion: 0,
                patchVersion: 0,
            },
        );
        add.setBezelStyle(if glass {
            NSBezelStyle::Glass
        } else {
            NSBezelStyle::Push
        });
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
                .constraintEqualToAnchor_constant(&safe_area.leadingAnchor(), 8.0),
            scroll
                .trailingAnchor()
                .constraintEqualToAnchor_constant(&safe_area.trailingAnchor(), -8.0),
            scroll
                .topAnchor()
                .constraintEqualToAnchor_constant(&safe_area.topAnchor(), 1.0),
            scroll
                .bottomAnchor()
                .constraintEqualToAnchor_constant(&add.topAnchor(), -10.0),
            add.leadingAnchor()
                .constraintEqualToAnchor_constant(&safe_area.leadingAnchor(), 10.0),
            add.trailingAnchor()
                .constraintEqualToAnchor_constant(&safe_area.trailingAnchor(), -10.0),
            add.bottomAnchor()
                .constraintEqualToAnchor_constant(&safe_area.bottomAnchor(), -10.0),
            add.heightAnchor().constraintEqualToConstant(36.0),
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
        // Accent and focus changes can leave backgroundStyle unchanged. Recolour
        // existing visible cells without recreating rows or moving the selection.
        self.table.refresh_cells();
        self.table.ivars().synchronizing.set(false);
    }
}

fn label(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let text = NSTextField::labelWithString(&NSString::new(), mtm);
    text.setTranslatesAutoresizingMaskIntoConstraints(false);
    text.setMaximumNumberOfLines(1);
    if let Some(cell) = text.cell() {
        cell.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    }
    text
}

fn group_cell(table: &NSTableView, mtm: MainThreadMarker) -> Retained<NSTableCellView> {
    let identifier = NSString::from_str("group");
    // SAFETY: Only this factory assigns this identifier to NSTableCellView.
    if let Some(view) = unsafe { table.makeViewWithIdentifier_owner(&identifier, None) } {
        return unsafe { Retained::cast_unchecked(view) };
    }
    let cell = NSTableCellView::new(mtm);
    cell.setIdentifier(Some(&identifier));
    let text = label(mtm);
    text.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    text.setTextColor(Some(&NSColor::secondaryLabelColor()));
    cell.addSubview(&text);
    // SAFETY: The native label is retained by the cell's subview hierarchy.
    unsafe {
        cell.setTextField(Some(&text));
    }
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        text.centerYAnchor()
            .constraintEqualToAnchor(&cell.centerYAnchor()),
        text.leadingAnchor()
            .constraintEqualToAnchor_constant(&cell.leadingAnchor(), 6.0),
        text.trailingAnchor()
            .constraintEqualToAnchor_constant(&cell.trailingAnchor(), -6.0),
    ]));
    cell
}

fn machine_cell(table: &NSTableView, mtm: MainThreadMarker) -> Retained<MachineCell> {
    let identifier = NSString::from_str("machine");
    // SAFETY: Only this factory assigns this identifier to MachineCell instances.
    if let Some(view) = unsafe { table.makeViewWithIdentifier_owner(&identifier, None) } {
        return unsafe { Retained::cast_unchecked(view) };
    }
    let metadata = label(mtm);
    metadata.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    metadata.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let status = NSImageView::new(mtm);
    status.setTranslatesAutoresizingMaskIntoConstraints(false);
    status.setImage(
        NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str("circle.fill"),
            None,
        )
        .as_deref(),
    );
    let allocated = MachineCell::alloc(mtm).set_ivars(CellLabels {
        metadata,
        status,
        selected: Cell::new(false),
        artwork_tint: Cell::new(None),
    });
    // SAFETY: NSTableCellView's designated initializer initializes this subclass.
    let cell: Retained<MachineCell> =
        unsafe { msg_send![super(allocated), initWithFrame: NSRect::ZERO] };
    cell.setIdentifier(Some(&identifier));
    let text = label(mtm);
    // SAFETY: AppKit exports the immutable medium system font weight.
    text.setFont(Some(&NSFont::systemFontOfSize_weight(13.0, unsafe {
        NSFontWeightMedium
    })));
    text.setTextColor(Some(&NSColor::labelColor()));
    cell.addSubview(&text);
    cell.addSubview(&cell.ivars().metadata);
    let region = NSView::new(mtm);
    region.setTranslatesAutoresizingMaskIntoConstraints(false);
    cell.addSubview(&region);
    let image = NSImageView::new(mtm);
    image.setTranslatesAutoresizingMaskIntoConstraints(false);
    image.setImageScaling(NSImageScaling::ScaleProportionallyDown);
    region.addSubview(&image);
    region.addSubview(&cell.ivars().status);
    // SAFETY: Both standard cell fields are retained by their subview hierarchy.
    unsafe {
        cell.setTextField(Some(&text));
        cell.setImageView(Some(&image));
    }
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        region
            .leadingAnchor()
            .constraintEqualToAnchor_constant(&cell.leadingAnchor(), 10.0),
        region
            .centerYAnchor()
            .constraintEqualToAnchor(&cell.centerYAnchor()),
        region.widthAnchor().constraintEqualToConstant(32.0),
        region.heightAnchor().constraintEqualToConstant(32.0),
        image
            .centerXAnchor()
            .constraintEqualToAnchor(&region.centerXAnchor()),
        image
            .centerYAnchor()
            .constraintEqualToAnchor(&region.centerYAnchor()),
        image.widthAnchor().constraintEqualToConstant(20.0),
        image.heightAnchor().constraintEqualToConstant(20.0),
        cell.ivars()
            .status
            .trailingAnchor()
            .constraintEqualToAnchor_constant(&region.trailingAnchor(), 2.0),
        cell.ivars()
            .status
            .bottomAnchor()
            .constraintEqualToAnchor_constant(&region.bottomAnchor(), 2.0),
        cell.ivars()
            .status
            .widthAnchor()
            .constraintEqualToConstant(9.0),
        cell.ivars()
            .status
            .heightAnchor()
            .constraintEqualToConstant(9.0),
        text.leadingAnchor()
            .constraintEqualToAnchor_constant(&region.trailingAnchor(), 10.0),
        text.trailingAnchor()
            .constraintEqualToAnchor_constant(&cell.trailingAnchor(), -10.0),
        text.centerYAnchor()
            .constraintEqualToAnchor_constant(&cell.centerYAnchor(), -7.0),
        cell.ivars()
            .metadata
            .leadingAnchor()
            .constraintEqualToAnchor(&text.leadingAnchor()),
        cell.ivars()
            .metadata
            .trailingAnchor()
            .constraintEqualToAnchor(&text.trailingAnchor()),
        cell.ivars()
            .metadata
            .topAnchor()
            .constraintEqualToAnchor_constant(&text.bottomAnchor(), 2.0),
    ]));
    cell
}

fn template_svg(bytes: &[u8]) -> Option<Retained<NSImage>> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(64, 64)?;
    let scale = (64.0 / size.width()).min(64.0 / size.height());
    let transform = resvg::tiny_skia::Transform::from_row(
        scale,
        0.0,
        0.0,
        scale,
        (64.0 - size.width() * scale) / 2.0,
        (64.0 - size.height() * scale) / 2.0,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let png = pixmap.encode_png().ok()?;
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&png))?;
    image.setSize(NSSize::new(32.0, 32.0));
    image.setTemplate(true);
    Some(image)
}
