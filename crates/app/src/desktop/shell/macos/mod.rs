//! AppKit split-view navigation and an unstyled system toolbar.
mod sidebar;
mod toolbar;

use crate::Crabdash;
use gpui::{App, Context, Window, px};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSLayoutConstraint, NSSplitView, NSSplitViewController,
    NSSplitViewDidResizeSubviewsNotification, NSSplitViewDividerStyle, NSSplitViewItem,
    NSTitlebarSeparatorStyle, NSView, NSViewController, NSWindow, NSWindowTitleVisibility,
};
use objc2_foundation::{
    NSArray, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSPoint,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use smol::channel::Sender;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use uuid::Uuid;

pub(super) enum Command {
    ToggleSidebar,
    Refresh,
    Terminal,
    Workspaces,
    AddMachine,
    SelectMachine(Uuid),
    RefreshMachine(Uuid),
    RenameMachine(Uuid),
    DeleteMachine(Uuid),
    SidebarGeometry { collapsed: bool, width: f32 },
}

pub(crate) fn commands_blocked(app: &Crabdash) -> bool {
    app.preferences_open
        || app.add_machine_modal_open
        || app.machine_rename.target.is_some()
        || app.docker_run_modal_open
        || app.docker_removal.is_some()
        || app.open_menu.is_some()
}

define_class!(
    // SAFETY: NSSplitViewController has no additional subclass requirements.
    #[unsafe(super = NSSplitViewController)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Sender<Command>]
    struct SplitController;
    unsafe impl NSObjectProtocol for SplitController {}
    impl SplitController {
        #[unsafe(method(loadView))]
        fn load_view(&self) {
            let split = self.splitView();
            // SAFETY: NSView initializes the pass-through controller root on
            // AppKit's normal loadView -> viewDidLoad lifecycle. Setting view
            // externally skips NSSplitViewController's native item setup.
            let plane: Retained<NavigationPlane> = unsafe {
                msg_send![super(NavigationPlane::alloc(self.mtm()).set_ivars(())), initWithFrame: split.frame()]
            };
            plane.addSubview(&split);
            split.setTranslatesAutoresizingMaskIntoConstraints(false);
            NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
                split.leadingAnchor().constraintEqualToAnchor(&plane.leadingAnchor()),
                split.trailingAnchor().constraintEqualToAnchor(&plane.trailingAnchor()),
                split.topAnchor().constraintEqualToAnchor(&plane.topAnchor()),
                split.bottomAnchor().constraintEqualToAnchor(&plane.bottomAnchor()),
            ]));
            self.setView(&plane);
        }

        #[unsafe(method(toggleSidebar:))]
        fn toggle_sidebar(&self, _: Option<&NSObject>) {
            // The built-in toolbar item uses the normal responder chain. Queue
            // the domain action so keyboard, toolbar and saved layouts agree.
            let _ = self.ivars().try_send(Command::ToggleSidebar);
        }
    }
);

define_class!(
    // SAFETY: A regular NSViewController routes the stock toolbar's responder
    // action to the native split controller's shared domain command.
    #[unsafe(super = NSViewController)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Sender<Command>]
    struct ContentController;
    unsafe impl NSObjectProtocol for ContentController {}
    impl ContentController {
        #[unsafe(method(toggleSidebar:))]
        fn toggle_sidebar(&self, _: Option<&NSObject>) {
            let _ = self.ivars().try_send(Command::ToggleSidebar);
        }
    }
);

define_class!(
    // SAFETY: Retain the controller's normal native descendants, but let the
    // empty navigation hosting plane pass input to the renderer below it.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct NavigationPlane;
    unsafe impl NSObjectProtocol for NavigationPlane {}
    impl NavigationPlane {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            // SAFETY: NSView performs ordinary native descendant hit testing.
            let hit: Option<Retained<NSView>> = unsafe { msg_send![super(self), hitTest: point] };
            hit.filter(|view| Retained::as_ptr(view).cast::<()>() != (self as *const Self).cast())
        }
    }
);

define_class!(
    // SAFETY: The detail plane is an empty NSView. Dashboard input belongs to
    // the full-width renderer immediately below this native split view.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct DetailPlane;
    unsafe impl NSObjectProtocol for DetailPlane {}
    impl DetailPlane {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _: NSPoint) -> Option<Retained<NSView>> { None }
    }
);

define_class!(
    // SAFETY: Keep AppKit's divider dragging and sidebar hit testing, while
    // allowing the otherwise empty dashboard detail to pass input through.
    #[unsafe(super = NSSplitView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<Option<Retained<NSView>>>]
    struct NavigationSplit;
    unsafe impl NSObjectProtocol for NavigationSplit {}
    impl NavigationSplit {
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            // SAFETY: Forward native descendant and divider hit testing.
            let hit: Option<Retained<NSView>> = unsafe { msg_send![super(self), hitTest: point] };
            let detail = self.ivars().borrow().clone();
            // SAFETY: The retained detail remains alive on the UI thread.
            let wrapper = detail.and_then(|detail| unsafe { detail.superview() });
            // Preserve AppKit's effective divider grab region exactly. Only
            // empty detail hits pass through; the split itself owns resizing.
            hit.filter(|hit| wrapper.as_ref().is_none_or(|wrapper| !hit.isDescendantOf(wrapper)))
        }
    }
);

struct Geometry {
    sidebar: Retained<NSSplitViewItem>,
    applying: Rc<Cell<bool>>,
    commands: Sender<Command>,
}
define_class!(
    // SAFETY: Notifications only queue work; they never reenter GPUI.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Geometry]
    struct GeometryTarget;
    unsafe impl NSObjectProtocol for GeometryTarget {}
    impl GeometryTarget {
        #[unsafe(method(crabdashSplitDidResize:))]
        fn resized(&self, _: Option<&NSNotification>) {
            let state = self.ivars();
            if state.applying.get() { return; }
            let width = state.sidebar.viewController(self.mtm()).view().frame().size.width as f32;
            let _ = state.commands.try_send(Command::SidebarGeometry {
                collapsed: state.sidebar.isCollapsed(), width,
            });
        }
    }
);

struct Observer {
    center: Retained<NSNotificationCenter>,
    target: Retained<GeometryTarget>,
}
impl Drop for Observer {
    fn drop(&mut self) {
        // SAFETY: This center registered precisely this retained observer.
        unsafe {
            self.center.removeObserver(&self.target);
        }
    }
}

/// Original GPUI host, retained so switching appearance preserves the window
/// and every dashboard/terminal entity rather than opening a replacement.
struct SharedHost {
    content: Retained<NSView>,
    controller: Option<Retained<NSViewController>>,
    constraints: Retained<NSArray<NSLayoutConstraint>>,
    autoresizing: NSAutoresizingMaskOptions,
    translates: bool,
    title_visibility: NSWindowTitleVisibility,
    separator: NSTitlebarSeparatorStyle,
}

pub(crate) struct State {
    id: Uuid,
    shared_host: SharedHost,
    split: Retained<SplitController>,
    host: Retained<ContentController>,
    drawer_inset: Retained<NSLayoutConstraint>,
    drawer_height: Cell<f32>,
    sidebar_item: Retained<NSSplitViewItem>,
    detail_view: Retained<NSView>,
    sidebar: sidebar::Sidebar,
    toolbar: toolbar::Toolbar,
    applying: Rc<Cell<bool>>,
    geometry: Cell<(bool, f32)>,
    overlay_open: Cell<bool>,
    _observer: Observer,
}

impl State {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Crabdash>) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let (renderer, native) = native_views(window)?;
        let shared_content = native.contentView()?;
        let original_controller = native.contentViewController();
        let autoresizing = renderer.autoresizingMask();
        let translates = renderer.translatesAutoresizingMaskIntoConstraints();
        let title_visibility = native.titleVisibility();
        let separator = native.titlebarSeparatorStyle();
        let original_frame = native.frame();
        let content_frame = renderer.frame();
        let (commands, receiver) = smol::channel::unbounded();
        let sidebar = sidebar::Sidebar::new(commands.clone(), mtm);
        let allocated = SplitController::alloc(mtm).set_ivars(commands.clone());
        // SAFETY: Initializes the retained NSSplitViewController subclass.
        let split: Retained<SplitController> = unsafe { msg_send![super(allocated), init] };
        // SAFETY: NSSplitView initializes the empty navigation subclass.
        let navigation: Retained<NavigationSplit> = unsafe {
            msg_send![super(NavigationSplit::alloc(mtm).set_ivars(RefCell::new(None))), initWithFrame: content_frame]
        };
        split.setSplitView(&navigation);
        split.splitView().setVertical(true);
        // Match NSSplitViewController's stock split. A standalone NSSplitView
        // starts with Thick, which has different divider metrics and grab zones.
        split
            .splitView()
            .setDividerStyle(NSSplitViewDividerStyle::Thin);
        let toolbar = toolbar::Toolbar::new(commands.clone(), split.splitView(), mtm);

        let sidebar_item = NSSplitViewItem::sidebarWithViewController(sidebar.controller());
        sidebar_item.setMinimumThickness(180.0);
        sidebar_item.setMaximumThickness(420.0);
        sidebar_item.setCanCollapse(true);

        let detail = NSViewController::new(mtm);
        // SAFETY: NSView initializes the pass-through detail subclass.
        let detail_view: Retained<DetailPlane> = unsafe {
            msg_send![super(DetailPlane::alloc(mtm).set_ivars(())), initWithFrame: content_frame]
        };
        detail.setView(&detail_view);
        *navigation.ivars().borrow_mut() = Some(detail_view.clone().into_super());
        // Register both items before loading the custom split view. AppKit's
        // first constraint pass asks the controller about every divider, so
        // its item model must already match the managed navigation children.
        let detail_item = NSSplitViewItem::splitViewItemWithViewController(&detail);
        detail_item.setMinimumThickness(360.0);
        split.addSplitViewItem(&sidebar_item);
        split.addSplitViewItem(&detail_item);
        let allocated = ContentController::alloc(mtm).set_ivars(commands.clone());
        // SAFETY: NSViewController initializes the retained content host.
        let host: Retained<ContentController> = unsafe { msg_send![super(allocated), init] };
        let host_view = NSView::initWithFrame(NSView::alloc(mtm), content_frame);
        host.setView(&host_view);
        host.addChildViewController(&split);
        let render_host = NSView::initWithFrame(NSView::alloc(mtm), content_frame);
        host_view.addSubview(&render_host);
        render_host.setTranslatesAutoresizingMaskIntoConstraints(false);
        renderer.removeFromSuperview();
        // GPUI recognizes an embedded renderer by its parent differing from
        // NSWindow.contentView and defers resize callbacks until App is unborrowed.
        render_host.addSubview(&renderer);
        renderer.setTranslatesAutoresizingMaskIntoConstraints(false);
        let safe = host_view.safeAreaLayoutGuide();
        // GPUI owns one full-width safe content plane. Native navigation is
        // overlaid only over the dashboard, so the terminal can reach both edges.
        let split_view = split.view();
        host_view.addSubview(&split_view);
        split_view.setTranslatesAutoresizingMaskIntoConstraints(false);
        let drawer_inset = split_view
            .bottomAnchor()
            .constraintEqualToAnchor(&safe.bottomAnchor());
        let constraints = [
            render_host
                .leadingAnchor()
                .constraintEqualToAnchor(&safe.leadingAnchor()),
            render_host
                .trailingAnchor()
                .constraintEqualToAnchor(&safe.trailingAnchor()),
            render_host
                .topAnchor()
                .constraintEqualToAnchor(&safe.topAnchor()),
            render_host
                .bottomAnchor()
                .constraintEqualToAnchor(&safe.bottomAnchor()),
            renderer
                .leadingAnchor()
                .constraintEqualToAnchor(&render_host.leadingAnchor()),
            renderer
                .trailingAnchor()
                .constraintEqualToAnchor(&render_host.trailingAnchor()),
            renderer
                .topAnchor()
                .constraintEqualToAnchor(&render_host.topAnchor()),
            renderer
                .bottomAnchor()
                .constraintEqualToAnchor(&render_host.bottomAnchor()),
            split_view
                .leadingAnchor()
                .constraintEqualToAnchor(&safe.leadingAnchor()),
            split_view
                .trailingAnchor()
                .constraintEqualToAnchor(&safe.trailingAnchor()),
            split_view
                .topAnchor()
                .constraintEqualToAnchor(&safe.topAnchor()),
            drawer_inset.clone(),
        ];
        let constraints = NSArray::from_retained_slice(&constraints);
        NSLayoutConstraint::activateConstraints(&constraints);
        native.setContentViewController(Some(&host));
        native.setToolbar(Some(toolbar.toolbar()));
        native.setTitleVisibility(NSWindowTitleVisibility::Visible);
        native.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::Line);
        native.setFrame_display(original_frame, false);
        host_view.layoutSubtreeIfNeeded();
        native.makeFirstResponder(Some(&renderer));
        window.refresh_native_viewport(cx);

        let applying = Rc::new(Cell::new(false));
        let allocated = GeometryTarget::alloc(mtm).set_ivars(Geometry {
            sidebar: sidebar_item.clone(),
            applying: applying.clone(),
            commands,
        });
        // SAFETY: NSObject initializes the retained notification target.
        let target: Retained<GeometryTarget> = unsafe { msg_send![super(allocated), init] };
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: The target is retained and its selector accepts NSNotification.
        unsafe {
            center.addObserver_selector_name_object(
                &target,
                sel!(crabdashSplitDidResize:),
                Some(NSSplitViewDidResizeSubviewsNotification),
                Some(&split.splitView()),
            );
        }
        let id = Uuid::new_v4();
        let handle = Window::window_handle(window);
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            while let Ok(command) = receiver.recv().await {
                let result = handle.update(cx, |_, window, cx| {
                    owner.update(cx, |app, cx| {
                        // A replaced shell may still have buffered commands. Only
                        // the exact currently installed shell owns this window.
                        if app
                            .native_shell
                            .as_ref()
                            .is_some_and(|shell| shell.id == id)
                        {
                            app.native_shell_command(command, window, cx);
                        }
                    })
                });
                if !matches!(result, Ok(Ok(()))) {
                    break;
                }
            }
        })
        .detach();
        Some(Self {
            id,
            shared_host: SharedHost {
                content: shared_content,
                controller: original_controller,
                constraints,
                autoresizing,
                translates,
                title_visibility,
                separator,
            },
            split,
            host,
            drawer_inset,
            drawer_height: Cell::new(0.0),
            sidebar_item,
            detail_view: detail_view.into_super(),
            sidebar,
            toolbar,
            applying,
            geometry: Cell::new((false, f32::NAN)),
            overlay_open: Cell::new(false),
            _observer: Observer { center, target },
        })
    }

    fn restore_shared_host(self, window: &mut Window, cx: &mut App) {
        let Some((renderer, native)) = native_views(window) else {
            return;
        };
        self.applying.set(true);
        let frame = native.frame();
        native.setToolbar(None);
        NSLayoutConstraint::deactivateConstraints(&self.shared_host.constraints);
        renderer.removeFromSuperview();
        native.setContentViewController(self.shared_host.controller.as_deref());
        native.setContentView(Some(&self.shared_host.content));
        native.setTitleVisibility(self.shared_host.title_visibility);
        native.setTitlebarSeparatorStyle(self.shared_host.separator);
        native.setFrame_display(frame, false);
        self.shared_host.content.layoutSubtreeIfNeeded();
        renderer.setTranslatesAutoresizingMaskIntoConstraints(self.shared_host.translates);
        renderer.setAutoresizingMask(self.shared_host.autoresizing);
        // Resize while unparented so GPUI queues the native viewport callback;
        // its original direct-content resize callback cannot reenter this render.
        renderer.setFrame(self.shared_host.content.bounds());
        self.shared_host.content.addSubview(&renderer);
        native.makeFirstResponder(Some(&renderer));
        window.refresh_native_viewport(cx);
    }

    pub(crate) fn synchronize(&self, app: &Crabdash, window: &mut Window, cx: &mut App) {
        let overlay_open = commands_blocked(app) || app.workspaces.open;
        if overlay_open && !self.overlay_open.replace(overlay_open) {
            if let Some((renderer, native)) = native_views(window) {
                native.makeFirstResponder(Some(&renderer));
            }
        } else {
            self.overlay_open.set(overlay_open);
        }
        self.sidebar.synchronize(app);
        self.toolbar.synchronize(app);
        let drawer_height = if app.quake_terminal_open && app.active_quake_terminal().is_some() {
            f32::from(app.quake_height).clamp(
                0.0,
                self.host
                    .view()
                    .safeAreaLayoutGuide()
                    .frame()
                    .size
                    .height
                    .max(0.0) as f32,
            )
        } else {
            0.0
        };
        if self.drawer_height.replace(drawer_height) != drawer_height {
            self.applying.set(true);
            // AppKit's bottom anchor increases downwards; a negative inset
            // raises native navigation above the full-width terminal drawer.
            self.drawer_inset.setConstant(-f64::from(drawer_height));
            self.host.view().layoutSubtreeIfNeeded();
            self.applying.set(false);
        }
        let desired = (app.sidebar_collapsed, f32::from(app.sidebar_width));
        if self.geometry.get() != desired {
            self.applying.set(true);
            self.sidebar_item.setCollapsed(desired.0);
            if !desired.0 {
                self.split
                    .splitView()
                    .setPosition_ofDividerAtIndex(f64::from(desired.1), 0);
            }
            self.split.view().layoutSubtreeIfNeeded();
            self.applying.set(false);
            self.geometry.set(desired);
            window.refresh_native_viewport(cx);
        }
    }

    pub(crate) fn sidebar_inset(&self) -> gpui::Pixels {
        if self.sidebar_item.isCollapsed() {
            return px(0.0);
        }
        // AppKit may overlay the visual divider inside the sidebar rather
        // than allocating dividerThickness between items. Use its laid-out
        // detail origin so dashboard content and the native grab seam agree.
        let origin = self
            .split
            .splitView()
            .convertPoint_fromView(NSPoint::new(0.0, 0.0), Some(&self.detail_view));
        px(origin.x as f32)
    }
}

fn native_views(window: &Window) -> Option<(Retained<NSView>, Retained<NSWindow>)> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: GPUI supplies its live NSView on the main thread. Retain it before
    // replacing the content view so reparenting cannot release the renderer.
    let renderer = unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) }?;
    let native = renderer.window()?;
    Some((renderer, native))
}

impl Crabdash {
    pub(crate) fn install_native_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.synchronize_native_shell(window, cx);
    }

    pub(crate) fn synchronize_native_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.preferences.liquid_glass {
            if let Some(shell) = self.native_shell.take() {
                shell.restore_shared_host(window, cx);
            }
            return;
        }
        if self.native_shell.is_none() {
            self.native_shell = State::new(window, cx);
            if self.native_shell.is_none() {
                tracing::error!("Unable to install the native macOS navigation shell");
            }
        }
        if let Some(shell) = self.native_shell.take() {
            shell.synchronize(self, window, cx);
            self.native_shell = Some(shell);
        }
    }

    fn native_shell_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Ignore actions queued by a native shell that has been removed.
        if self.native_shell.is_none() {
            return;
        }
        // Native divider layout has already happened. Persist it even while a
        // popup is open, without stealing keyboard focus from its text fields.
        if let Command::SidebarGeometry { collapsed, width } = command {
            // A burst of native drag notifications can queue intermediate
            // widths. Reconcile the latest physical layout, never drag back to
            // an older snapshot when the asynchronous command is delivered.
            let (collapsed, width) =
                self.native_shell
                    .as_ref()
                    .map_or((collapsed, width), |shell| {
                        (
                            shell.sidebar_item.isCollapsed(),
                            shell.sidebar.controller().view().frame().size.width as f32,
                        )
                    });
            let collapsed_changed = self.sidebar_collapsed != collapsed;
            if collapsed_changed {
                self.sidebar_collapsed = collapsed;
            }
            if !collapsed
                && width.is_finite()
                && width >= 180.0
                && (f32::from(self.sidebar_width) - width).abs() >= 0.5
            {
                self.set_sidebar_width(px(width), cx);
            } else if collapsed_changed {
                self.persist_workspace(cx);
                cx.notify();
            }
            return;
        }
        if commands_blocked(self) {
            return;
        }
        if self.workspaces.open
            && matches!(
                command,
                Command::AddMachine
                    | Command::SelectMachine(_)
                    | Command::RefreshMachine(_)
                    | Command::RenameMachine(_)
                    | Command::DeleteMachine(_)
            )
        {
            return;
        }
        if matches!(
            command,
            Command::ToggleSidebar
                | Command::Terminal
                | Command::Workspaces
                | Command::AddMachine
                | Command::RenameMachine(_)
        ) && let Some((renderer, native)) = native_views(window)
        {
            native.makeFirstResponder(Some(&renderer));
        }
        match command {
            Command::ToggleSidebar => self.toggle_sidebar(cx),
            Command::Refresh => {
                self.refresh_services(cx);
                cx.notify();
            }
            Command::Terminal => self.toggle_quake_terminal(window, cx),
            Command::Workspaces => self.toggle_workspace_popup(window, cx),
            Command::AddMachine => self.open_add_machine_modal(window, cx),
            Command::SelectMachine(id) => self.select_machine(id, window, cx),
            Command::RefreshMachine(id) => self.refresh_machine(id, cx),
            Command::RenameMachine(id) => self.open_machine_rename(id, window, cx),
            Command::DeleteMachine(id) => {
                if self
                    .machine_store
                    .machines
                    .iter()
                    .any(|machine| machine.uuid == id && machine.id != "localhost")
                {
                    self.delete_machine(id, window, cx);
                }
            }
            Command::SidebarGeometry { .. } => unreachable!("geometry handled before actions"),
        }
    }
}
