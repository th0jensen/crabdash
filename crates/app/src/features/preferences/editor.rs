use crate::{
    app::Crabdash,
    components::{
        common::{button, chrome_icon_button},
        style,
        text_field::TextField,
    },
    features::preferences::Preferences,
};
use anyhow::{Context as _, Result, bail};
use gpui::{prelude::*, *};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Section {
    #[default]
    General,
    Terminal,
    Interface,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Field {
    Refresh,
    SystemRefresh,
    LogLines,
    TerminalFont,
    TerminalSize,
    LineHeight,
    Terminfo,
    Scrollback,
    TerminalRows,
    InterfaceFont,
    InterfaceSize,
    SidebarWidth,
    TabWidth,
}
impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Refresh => "Refresh interval",
            Self::SystemRefresh => "Sample interval",
            Self::LogLines => "Recent log lines",
            Self::TerminalFont | Self::InterfaceFont => "Font family",
            Self::TerminalSize | Self::InterfaceSize => "Font size",
            Self::LineHeight => "Line height",
            Self::Terminfo => "Terminal type / terminfo",
            Self::Scrollback => "Scrollback buffer",
            Self::TerminalRows => "Initial panel height",
            Self::SidebarWidth => "Sidebar width",
            Self::TabWidth => "Maximum terminal tab width",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Self::Refresh => "Seconds · 2–300",
            Self::SystemRefresh => "Seconds · 2–60",
            Self::LogLines => "50–10,000 lines · newly opened logs",
            Self::TerminalFont => "An installed monospaced font; bundled font works everywhere",
            Self::InterfaceFont => "Leave empty to use the system font",
            Self::TerminalSize => "Pixels · 8–32",
            Self::InterfaceSize => "Pixels · 10–20; controls scale together",
            Self::LineHeight => "Font-size multiplier · 1–2.5",
            Self::Terminfo => "xterm-256color is recommended for local and SSH shells",
            Self::Scrollback => "0–100,000 lines · new terminal sessions",
            Self::TerminalRows => "4–48 rows · initial height",
            Self::SidebarWidth => "Pixels · 180–420",
            Self::TabWidth => "Pixels · 120–240 maximum; long terminal titles use ellipses",
        }
    }
}

/// Every declared field owns an input; exhaustive matching prevents missing-field panics.
struct Inputs {
    refresh: Entity<TextField>,
    log_lines: Entity<TextField>,
    terminal_font: Entity<TextField>,
    terminal_size: Entity<TextField>,
    line_height: Entity<TextField>,
    terminfo: Entity<TextField>,
    scrollback: Entity<TextField>,
    terminal_rows: Entity<TextField>,
    interface_font: Entity<TextField>,
    interface_size: Entity<TextField>,
    sidebar_width: Entity<TextField>,
    tab_width: Entity<TextField>,
    system_refresh: Entity<TextField>,
}
impl Inputs {
    fn new(settings: &Preferences, cx: &mut Context<Crabdash>) -> Self {
        let mut input = |field: Field, value: String, index: isize| {
            cx.new(|cx| {
                let mut input = TextField::new(
                    "",
                    if field == Field::InterfaceFont {
                        "System font"
                    } else {
                        ""
                    },
                    index,
                    cx,
                );
                input.set_text(&value, cx);
                input
            })
        };
        Self {
            refresh: input(Field::Refresh, settings.refresh_seconds.to_string(), 1),
            log_lines: input(Field::LogLines, settings.log_lines.to_string(), 2),
            terminal_font: input(Field::TerminalFont, settings.terminal_font.clone(), 3),
            terminal_size: input(
                Field::TerminalSize,
                settings.terminal_font_size.to_string(),
                4,
            ),
            line_height: input(
                Field::LineHeight,
                settings.terminal_line_height.to_string(),
                5,
            ),
            terminfo: input(Field::Terminfo, settings.terminal_type.clone(), 6),
            scrollback: input(Field::Scrollback, settings.scrollback_lines.to_string(), 7),
            terminal_rows: input(Field::TerminalRows, settings.terminal_rows.to_string(), 8),
            interface_font: input(Field::InterfaceFont, settings.interface_font.clone(), 9),
            interface_size: input(
                Field::InterfaceSize,
                settings.interface_font_size.to_string(),
                10,
            ),
            sidebar_width: input(Field::SidebarWidth, settings.sidebar_width.to_string(), 11),
            tab_width: input(Field::TabWidth, settings.tab_width.to_string(), 12),
            system_refresh: input(
                Field::SystemRefresh,
                settings.system_refresh_seconds.to_string(),
                13,
            ),
        }
    }
    fn get(&self, field: Field) -> &Entity<TextField> {
        match field {
            Field::Refresh => &self.refresh,
            Field::LogLines => &self.log_lines,
            Field::TerminalFont => &self.terminal_font,
            Field::TerminalSize => &self.terminal_size,
            Field::LineHeight => &self.line_height,
            Field::Terminfo => &self.terminfo,
            Field::Scrollback => &self.scrollback,
            Field::TerminalRows => &self.terminal_rows,
            Field::InterfaceFont => &self.interface_font,
            Field::InterfaceSize => &self.interface_size,
            Field::SidebarWidth => &self.sidebar_width,
            Field::TabWidth => &self.tab_width,
            Field::SystemRefresh => &self.system_refresh,
        }
    }
    fn all(&self) -> [(Field, &Entity<TextField>); 13] {
        [
            (Field::Refresh, &self.refresh),
            (Field::LogLines, &self.log_lines),
            (Field::TerminalFont, &self.terminal_font),
            (Field::TerminalSize, &self.terminal_size),
            (Field::LineHeight, &self.line_height),
            (Field::Terminfo, &self.terminfo),
            (Field::Scrollback, &self.scrollback),
            (Field::TerminalRows, &self.terminal_rows),
            (Field::InterfaceFont, &self.interface_font),
            (Field::InterfaceSize, &self.interface_size),
            (Field::SidebarWidth, &self.sidebar_width),
            (Field::TabWidth, &self.tab_width),
            (Field::SystemRefresh, &self.system_refresh),
        ]
    }
}

pub(crate) struct Editor {
    pub section: Section,
    search: Entity<TextField>,
    _search_changes: Subscription,
    pub draft: Preferences,
    fields: Inputs,
    pub busy: bool,
    pub error: Option<String>,
    pub mutation_error: Option<String>,
    pub scroll: ScrollHandle,
    pub picker: Option<Field>,
    choices: std::cell::RefCell<std::collections::HashMap<Field, Vec<String>>>,
}
impl Editor {
    pub fn new(settings: &Preferences, cx: &mut Context<Crabdash>) -> Self {
        let fields = Inputs::new(settings, cx);
        let search = cx.new(|cx| {
            TextField::new("", "Search settings…", 0, cx)
                .compact()
                .native_search()
        });
        let mut previous_query = String::new();
        let search_changes = cx.observe(&search, move |this, field, cx| {
            let query = field.read(cx).text().trim().to_lowercase();
            if query != previous_query {
                previous_query = query;
                this.preference_editor.picker = None;
                this.preference_editor
                    .scroll
                    .set_offset(point(px(0.0), px(0.0)));
                cx.notify();
            }
        });
        Self {
            section: Section::General,
            search,
            _search_changes: search_changes,
            draft: settings.clone(),
            fields,
            busy: false,
            error: None,
            mutation_error: None,
            scroll: ScrollHandle::new(),
            picker: None,
            choices: Default::default(),
        }
    }
    fn input(&self, field: Field) -> Entity<TextField> {
        self.fields.get(field).clone()
    }
    pub fn collect(&self, cx: &App) -> Result<Preferences> {
        let mut value = self.draft.clone();
        for (field, input) in self.fields.all() {
            let text = input.read(cx).text().trim().to_owned();
            macro_rules! number {
                ($target:expr) => {
                    $target = text
                        .parse()
                        .with_context(|| format!("{} must be a number.", field.label()))?
                };
            }
            match field {
                Field::Refresh => number!(value.refresh_seconds),
                Field::SystemRefresh => number!(value.system_refresh_seconds),
                Field::LogLines => number!(value.log_lines),
                Field::TerminalFont => value.terminal_font = text,
                Field::TerminalSize => number!(value.terminal_font_size),
                Field::LineHeight => number!(value.terminal_line_height),
                Field::Terminfo => value.terminal_type = text,
                Field::Scrollback => number!(value.scrollback_lines),
                Field::TerminalRows => number!(value.terminal_rows),
                Field::InterfaceFont => value.interface_font = text,
                Field::InterfaceSize => number!(value.interface_font_size),
                Field::SidebarWidth => number!(value.sidebar_width),
                Field::TabWidth => number!(value.tab_width),
            }
        }
        value.validate()?;
        let mut names = cx.text_system().all_font_names();
        names.push("JetBrainsMono Nerd Font".into());
        for family in [&value.terminal_font, &value.interface_font] {
            if !family.is_empty() && !names.iter().any(|name| name.eq_ignore_ascii_case(family)) {
                bail!(
                    "Font ‘{family}’ is not installed. Enter an installed font family or restore defaults."
                );
            }
        }
        let font_id = cx
            .text_system()
            .resolve_font(&font(value.terminal_font.clone()));
        let narrow = cx
            .text_system()
            .advance(font_id, px(value.terminal_font_size), 'i')?
            .width;
        let wide = cx
            .text_system()
            .advance(font_id, px(value.terminal_font_size), 'W')?
            .width;
        if (f32::from(wide) - f32::from(narrow)).abs() > 0.1 {
            bail!("Choose a monospaced terminal font so columns stay aligned.");
        }
        Ok(value)
    }
}

fn choice_values(editor: &Editor, field: Field, cx: &mut Context<Crabdash>) -> Vec<String> {
    if let Some(values) = editor.choices.borrow().get(&field) {
        return values.clone();
    }
    let mut choices = if matches!(field, Field::Terminfo) {
        vec![
            "xterm-256color".to_owned(),
            "xterm".to_owned(),
            "vt100".to_owned(),
            "xterm-ghostty".to_owned(),
        ]
    } else {
        let mut names = cx.text_system().all_font_names();
        names.push("JetBrainsMono Nerd Font".into());
        names.sort();
        names.dedup();
        if matches!(field, Field::TerminalFont) {
            names.retain(|name| {
                let id = cx.text_system().resolve_font(&font(name.clone()));
                match (
                    cx.text_system().advance(id, px(13.0), 'i'),
                    cx.text_system().advance(id, px(13.0), 'W'),
                ) {
                    (Ok(a), Ok(b)) => (f32::from(a.width) - f32::from(b.width)).abs() < 0.1,
                    _ => false,
                }
            });
        }
        names
    };
    if matches!(field, Field::InterfaceFont) {
        choices.insert(0, String::new());
    }
    editor.choices.borrow_mut().insert(field, choices.clone());
    choices
}

fn field_row(
    _app: &Crabdash,
    editor: &Editor,
    field: Field,
    compact: bool,
    cx: &mut Context<Crabdash>,
) -> Div {
    let selectable = matches!(
        field,
        Field::TerminalFont | Field::InterfaceFont | Field::Terminfo
    );
    let mut control = div()
        .w(gpui::rems(190.0 / 16.0))
        .when(compact, |this| this.w_full())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.0))
        .child(div().flex_1().min_w_0().child(editor.input(field)));
    if selectable {
        let chooser = button(
            SharedString::from(format!("choose-{field:?}")),
            Some(lucide_icons::Icon::ChevronDown),
            None::<&str>,
            false,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.preference_editor.picker = if this.preference_editor.picker == Some(field) {
                None
            } else {
                Some(field)
            };
            cx.notify();
        }));
        #[cfg(target_os = "macos")]
        let chooser = if _app.preferences.liquid_glass {
            crate::desktop::controls::enabled(
                crate::desktop::controls::choices(
                    chooser,
                    editor.input(field),
                    choice_values(editor, field, cx),
                    match field {
                        Field::TerminalFont => "Choose a terminal font",
                        Field::InterfaceFont => "Choose an interface font",
                        Field::Terminfo => "Choose a terminal type",
                        _ => field.label(),
                    },
                ),
                !editor.busy,
            )
        } else {
            chooser
        };
        control = control.child(chooser);
    }
    let mut result = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(row(field.label(), field.hint(), compact).child(control));
    if editor.picker == Some(field) {
        let choices = choice_values(editor, field, cx);
        result = result.child(
            div()
                .id(SharedString::from(format!("choices-{field:?}")))
                .max_h(px(160.0))
                .overflow_y_scroll()
                .border_1()
                .border_color(crate::components::style::rgb(0x383838))
                .bg(crate::components::style::rgb(0x181818))
                .rounded(px(style::RADIUS))
                .flex()
                .flex_col()
                .children(choices.into_iter().map(|choice| {
                    let label = if choice.is_empty() {
                        "System font".to_owned()
                    } else {
                        choice.clone()
                    };
                    div()
                        .id(SharedString::from(format!("choice-{field:?}-{label}")))
                        .px(px(12.0))
                        .py(px(7.0))
                        .text_size(gpui::rems(style::TEXT / 16.0))
                        .cursor_pointer()
                        .hover(|this| this.bg(crate::components::style::rgb(0x333333)))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.preference_editor
                                .input(field)
                                .update(cx, |input, cx| input.set_text(&choice, cx));
                            this.preference_editor.picker = None;
                            cx.notify();
                        }))
                })),
        );
    }
    result
}
fn row(label: &'static str, hint: &'static str, compact: bool) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(20.0))
        .when(compact, |this| this.flex_col().items_start().gap(px(10.0)))
        .py(px(16.0))
        .border_b_1()
        .border_color(style::rgb(style::BORDER))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(div().text_size(gpui::rems(style::TEXT / 16.0)).child(label))
                .child(
                    div()
                        .text_size(gpui::rems(style::META / 16.0))
                        .text_color(crate::components::style::rgb(style::TEXT_MUTED))
                        .whitespace_normal()
                        .child(hint),
                ),
        )
}
fn toggle_row(
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    enabled: bool,
    busy: bool,
    toggle: impl Fn(&mut Crabdash, &mut Context<Crabdash>) + 'static,
    cx: &mut Context<Crabdash>,
) -> Div {
    row(label, hint, false).child(crate::desktop::controls::enabled(
        crate::desktop::controls::toggle(
            div()
                .id(id)
                .w(px(38.0))
                .h(px(22.0))
                .flex_none()
                .px(px(3.0))
                .rounded_full()
                .flex()
                .items_center()
                .bg(crate::components::style::rgb(if enabled {
                    style::FOCUS_BORDER
                } else {
                    style::CONTROL_SELECTED_BG
                }))
                .when(enabled, |this| this.justify_end())
                .when(busy, |this| this.opacity(0.5))
                .cursor_pointer()
                .child(div().size(px(16.0)).rounded_full().bg(white()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !busy {
                        toggle(this, cx);
                        cx.notify();
                    }
                })),
            label,
            enabled,
        ),
        !busy,
    ))
}

// A setting's label and description also supply its search terms. Searching
// includes every category without changing or collecting the current draft.
impl Section {
    const ALL: [Self; 4] = [Self::General, Self::Interface, Self::Terminal, Self::System];
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Interface => "Interface",
            Self::Terminal => "Terminal",
            Self::System => "System",
        }
    }
    fn fields(self) -> &'static [Field] {
        match self {
            Self::General => &[Field::Refresh, Field::LogLines],
            Self::Interface => &[
                Field::InterfaceFont,
                Field::InterfaceSize,
                Field::SidebarWidth,
                Field::TabWidth,
            ],
            Self::Terminal => &[
                Field::TerminalFont,
                Field::TerminalSize,
                Field::LineHeight,
                Field::Terminfo,
                Field::Scrollback,
                Field::TerminalRows,
            ],
            Self::System => &[Field::SystemRefresh],
        }
    }
    fn toggles(self) -> &'static [Toggle] {
        match self {
            Self::General => &[
                Toggle::StartAtLogin,
                Toggle::StartMinimised,
                Toggle::CloseToTray,
                Toggle::AutoRefresh,
            ],
            Self::Interface => &[Toggle::LiquidGlass, Toggle::SystemAccent],
            Self::Terminal => &[Toggle::TrueColor],
            Self::System => &[],
        }
    }
    fn note(self) -> &'static str {
        match self {
            Self::General => {
                "Errors stay visible until dismissed. Refresh manually with the refresh button or shortcut."
            }
            Self::Interface => "Interface changes apply when you save your preferences.",
            Self::Terminal => {
                "Fonts apply to terminals and logs immediately. Terminal type, colour advertisement and scrollback apply to new sessions. xterm-ghostty requires its terminfo on the host."
            }
            Self::System => {
                "Resources update while System is visible, independently of automatic table refresh. Longer intervals reduce sampling and rendering work."
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Toggle {
    StartAtLogin,
    StartMinimised,
    CloseToTray,
    AutoRefresh,
    LiquidGlass,
    SystemAccent,
    TrueColor,
}
impl Toggle {
    fn details(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::StartAtLogin => (
                "start-at-login-toggle",
                "Start at login",
                "Open Crabdash at desktop login; saved immediately",
            ),
            Self::StartMinimised => (
                "start-minimised-toggle",
                "Start minimised",
                "Open in the background on the next launch",
            ),
            Self::CloseToTray => (
                "close-to-tray-toggle",
                "Keep running when closed",
                "Keep sessions alive when a system tray is available; Quit exits",
            ),
            Self::AutoRefresh => (
                "auto-refresh-toggle",
                "Automatic refresh",
                "Update machine status and tables",
            ),
            Self::LiquidGlass => (
                "liquid-glass-toggle",
                "Liquid Glass",
                "Use the native macOS sidebar and toolbar; off uses the shared interface",
            ),
            Self::SystemAccent => (
                "system-accent-toggle",
                "Use system accent",
                "Colour the focused tab, resource graphs and the shared sidebar",
            ),
            Self::TrueColor => (
                "true-color-toggle",
                "Advertise true colour",
                "Set COLORTERM=truecolor for new sessions when the host permits",
            ),
        }
    }
    fn supported(self) -> bool {
        match self {
            Self::StartAtLogin | Self::StartMinimised | Self::CloseToTray => {
                crate::desktop::startup::SUPPORTED && crate::desktop::tray::SUPPORTED
            }
            Self::LiquidGlass => cfg!(target_os = "macos"),
            _ => true,
        }
    }
    fn enabled(self, app: &Crabdash) -> bool {
        let draft = &app.preference_editor.draft;
        match self {
            Self::StartAtLogin => app.login_startup.enabled,
            Self::StartMinimised => draft.start_minimised,
            Self::CloseToTray => draft.close_to_tray,
            Self::AutoRefresh => draft.auto_refresh,
            Self::LiquidGlass => draft.liquid_glass,
            Self::SystemAccent => draft.use_system_accent,
            Self::TrueColor => draft.true_color,
        }
    }
    fn toggle(self, app: &mut Crabdash, cx: &mut Context<Crabdash>) {
        match self {
            Self::StartAtLogin => app.toggle_login_startup(cx),
            Self::StartMinimised => app.preference_editor.draft.start_minimised ^= true,
            Self::CloseToTray => app.preference_editor.draft.close_to_tray ^= true,
            Self::AutoRefresh => app.preference_editor.draft.auto_refresh ^= true,
            Self::LiquidGlass => app.preference_editor.draft.liquid_glass ^= true,
            Self::SystemAccent => app.preference_editor.draft.use_system_accent ^= true,
            Self::TrueColor => app.preference_editor.draft.true_color ^= true,
        }
    }
}
fn matches_query(query: &str, section: Section, label: &str, hint: &str) -> bool {
    let terms = format!("{} {label} {hint}", section.label()).to_lowercase();
    query.split_whitespace().all(|word| terms.contains(word))
}
fn section_matches(section: Section, query: &str) -> bool {
    section
        .fields()
        .iter()
        .any(|field| matches_query(query, section, field.label(), field.hint()))
        || section
            .toggles()
            .iter()
            .copied()
            .filter(|toggle| toggle.supported())
            .any(|toggle| {
                let (_, label, hint) = toggle.details();
                matches_query(query, section, label, hint)
            })
        || section == Section::Interface
            && matches_query(
                query,
                section,
                "Appearance",
                "Follow the system, or choose a light or dark interface",
            )
}
fn appearance_row(editor: &Editor, busy: bool, compact: bool, cx: &mut Context<Crabdash>) -> Div {
    row(
        "Appearance",
        "Follow the system, or choose a light or dark interface",
        compact,
    )
    .child(crate::desktop::controls::action_group(
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(rems(4.0 / 16.0)),
        [
            super::Appearance::System,
            super::Appearance::Light,
            super::Appearance::Dark,
        ]
        .into_iter()
        .map(|appearance| {
            let label = match appearance {
                super::Appearance::System => "System",
                super::Appearance::Light => "Light",
                super::Appearance::Dark => "Dark",
            };
            let selected = editor.draft.appearance == appearance;
            crate::desktop::controls::enabled(
                crate::desktop::controls::selected(
                    button(
                        SharedString::from(format!("appearance-{label}")),
                        None::<lucide_icons::Icon>,
                        Some(label),
                        selected,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !busy {
                            this.preference_editor.draft.appearance = appearance;
                            cx.notify();
                        }
                    })),
                    selected,
                ),
                !busy,
            )
        })
        .collect(),
    ))
}
fn section_body(
    app: &Crabdash,
    section: Section,
    query: &str,
    busy: bool,
    compact: bool,
    cx: &mut Context<Crabdash>,
) -> Div {
    let editor = &app.preference_editor;
    let mut body = div().flex().flex_col().min_w_0().child(
        div()
            .text_size(rems(16.0 / 16.0))
            .font_weight(FontWeight::SEMIBOLD)
            .mb(px(8.0))
            .child(section.label()),
    );
    if section == Section::Interface
        && matches_query(
            query,
            section,
            "Appearance",
            "Follow the system, or choose a light or dark interface",
        )
    {
        body = body.child(appearance_row(editor, busy, compact, cx));
    }
    for toggle in section
        .toggles()
        .iter()
        .copied()
        .filter(|toggle| toggle.supported())
    {
        let (id, label, hint) = toggle.details();
        if matches_query(query, section, label, hint) {
            body = body.child(toggle_row(
                id,
                label,
                hint,
                toggle.enabled(app),
                busy,
                move |app, cx| toggle.toggle(app, cx),
                cx,
            ));
        }
    }
    for field in section
        .fields()
        .iter()
        .copied()
        .filter(|field| matches_query(query, section, field.label(), field.hint()))
    {
        body = body.child(field_row(app, editor, field, compact, cx));
    }
    body.when(query.is_empty(), |this| {
        this.child(
            div()
                .pt(px(16.0))
                .text_size(rems(style::META / 16.0))
                .text_color(style::rgb(style::TEXT_MUTED))
                .whitespace_normal()
                .child(section.note()),
        )
    })
}

pub fn render(app: &Crabdash, window: &Window, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let editor = &app.preference_editor;
    let busy = editor.busy || app.startup_busy || super::mutation::is_busy(cx);
    let available_width =
        f32::from(window.viewport_size().width - crate::desktop::shell::native_sidebar_inset(app));
    let scale = f32::from(window.rem_size()) / 16.0;
    let wide = available_width >= 660.0 * scale;
    let compact = !wide;
    let query = editor.search.read(cx).text().trim().to_lowercase();
    let mut body = div().flex().flex_col().gap(px(28.0));
    let sections: Vec<_> = Section::ALL
        .into_iter()
        .filter(|section| {
            if query.is_empty() {
                *section == editor.section
            } else {
                section_matches(*section, &query)
            }
        })
        .collect();
    for section in sections.iter().copied() {
        body = body.child(section_body(app, section, &query, busy, compact, cx));
    }
    if sections.is_empty() {
        body = body.child(
            div()
                .py(px(24.0))
                .text_color(style::rgb(style::TEXT_MUTED))
                .whitespace_normal()
                .child("No matching settings. Try a font, appearance, terminal or startup option."),
        );
    }
    let categories = div()
        .flex()
        .gap(px(4.0))
        .when(wide, |this| this.flex_col())
        .children(Section::ALL.into_iter().map(|section| {
            let selected = query.is_empty() && editor.section == section;
            div()
                .id(SharedString::from(format!(
                    "preferences-{}",
                    section.label()
                )))
                .px(px(10.0))
                .py(px(7.0))
                .min_w_0()
                .rounded(px(style::RADIUS))
                .text_size(rems(style::TEXT / 16.0))
                .cursor_pointer()
                .when(wide, |this| this.w_full())
                .when(!wide, |this| this.flex_1().text_center())
                .when(selected, |this| {
                    this.bg(style::rgb(style::SURFACE_HOVER))
                        .text_color(style::rgb(style::TEXT_SELECTED))
                })
                .hover(|this| this.bg(style::rgb(style::SURFACE_HOVER)))
                .child(section.label())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.preference_editor.section = section;
                    this.preference_editor
                        .search
                        .update(cx, |search, cx| search.clear(cx));
                    this.preference_editor.picker = None;
                    this.preference_editor
                        .scroll
                        .set_offset(point(px(0.0), px(0.0)));
                    window.focus(&this.focus_handle);
                    cx.notify();
                }))
        }));
    let navigation = div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(12.0))
        .when(wide, |this| this.w(rems(180.0 / 16.0)).border_r_1())
        .when(!wide, |this| this.border_b_1())
        .border_color(style::rgb(style::BORDER))
        .child(editor.search.clone())
        .child(categories);
    let errors = visible_errors(
        editor.error.as_deref(),
        app.login_startup.error.as_deref(),
        editor.mutation_error.as_deref(),
        super::mutation::is_busy(cx),
    );
    let footer = div()
        .px(px(16.0))
        .py(px(12.0))
        .border_t_1()
        .border_color(style::rgb(style::BORDER))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .children(errors.into_iter().map(|error| {
            div()
                .text_size(rems(style::META / 16.0))
                .text_color(style::rgb(0xFF9F99))
                .whitespace_normal()
                .child(error)
        }))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    button(
                        "reset-preferences",
                        None::<lucide_icons::Icon>,
                        Some("Restore defaults"),
                        false,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.preference_editor.busy {
                            this.preference_editor = Editor::new(&Preferences::default(), cx);
                            window.focus(&this.focus_handle);
                            cx.notify();
                        }
                    })),
                )
                .child(crate::desktop::controls::action_group(
                    div().flex().gap(px(8.0)),
                    vec![
                        button(
                            "cancel-preferences",
                            None::<lucide_icons::Icon>,
                            Some("Cancel"),
                            false,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            if !this.preference_editor.busy {
                                this.preferences_open = false;
                                window.focus(&this.focus_handle);
                                cx.notify();
                            }
                        })),
                        crate::desktop::controls::enabled(
                            crate::desktop::controls::primary(button(
                                "save-preferences",
                                None::<lucide_icons::Icon>,
                                Some(if busy { "Saving…" } else { "Apply" }),
                                true,
                            ))
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.apply_preferences(window, cx)
                                }),
                            ),
                            !busy,
                        ),
                    ],
                )),
        );
    div()
        .absolute()
        .inset_0()
        .size_full()
        .bg(style::rgba(0x00000099))
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(crate::desktop::controls::modal(
            div()
                .w(px(820.0 * scale))
                .max_w_full()
                .h(px(
                    (f32::from(window.viewport_size().height) - 80.0).clamp(240.0, 680.0)
                ))
                .rounded(px(style::RADIUS))
                .bg(style::rgb(style::CONTENT))
                .border_1()
                .border_color(style::rgb(style::BORDER))
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(rems(40.0 / 16.0))
                        .flex_none()
                        .px(px(16.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(style::rgb(style::BORDER))
                        .child(
                            div()
                                .text_size(rems(style::TEXT / 16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Preferences"),
                        )
                        .child(
                            chrome_icon_button("close-preferences", lucide_icons::Icon::X)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if !this.preference_editor.busy {
                                        this.preferences_open = false;
                                        window.focus(&this.focus_handle);
                                        cx.notify();
                                    }
                                })),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .when(!wide, |this| this.flex_col())
                        .child(navigation)
                        .child(
                            div()
                                .id("preferences-scroll")
                                .flex_1()
                                .min_h_0()
                                .min_w_0()
                                .overflow_y_scroll()
                                .track_scroll(&editor.scroll)
                                .p(px(if wide { 28.0 } else { 16.0 }))
                                .child(body),
                        ),
                )
                .child(footer),
            0.6,
        ))
}

/// Native registration has its own result. It must remain visible beside
/// unrelated editor errors; a blocked operation is only relevant while busy.
fn visible_errors(
    editor_error: Option<&str>,
    startup_error: Option<&str>,
    mutation_error: Option<&str>,
    busy: bool,
) -> Vec<String> {
    let mut errors = Vec::new();
    for error in [editor_error, startup_error, mutation_error.filter(|_| busy)]
        .into_iter()
        .flatten()
    {
        if !errors.iter().any(|existing| existing == error) {
            errors.push(error.to_string());
        }
    }
    errors
}

#[cfg(test)]
mod error_tests {
    use super::visible_errors;
    use std::prelude::v1::test;

    #[test]
    fn native_failure_is_visible_beside_validation_and_completed_busy_errors() {
        assert_eq!(
            visible_errors(
                Some("Invalid font"),
                Some("Registration failed"),
                Some("Login startup is being updated"),
                false
            ),
            ["Invalid font", "Registration failed"]
        );
        assert_eq!(
            visible_errors(
                None,
                Some("Registration failed"),
                Some("Login startup is being updated"),
                true
            ),
            ["Registration failed", "Login startup is being updated"]
        );
    }

    #[test]
    fn persistent_errors_remain_after_idle_and_identical_messages_are_deduplicated() {
        assert_eq!(
            visible_errors(
                Some("Unable to read settings"),
                Some("Unable to read settings"),
                Some("Another window is saving"),
                false
            ),
            ["Unable to read settings"]
        );
        assert_eq!(
            visible_errors(Some("Unable to save settings"), None, None, false),
            ["Unable to save settings"]
        );
    }
}

#[cfg(test)]
mod search_tests {
    use super::{Section, matches_query, section_matches};
    use std::prelude::v1::test;

    #[test]
    fn search_finds_settings_in_other_categories_and_requires_every_word() {
        assert!(section_matches(Section::Terminal, "font"));
        assert!(section_matches(Section::Interface, "font"));
        assert!(!section_matches(Section::System, "font"));
        assert!(section_matches(Section::Interface, "dark"));
        assert!(section_matches(Section::Terminal, "terminfo"));
        assert!(section_matches(Section::General, "refresh interval"));
        assert!(!section_matches(Section::General, "refresh font"));
    }

    #[test]
    fn category_search_and_whitespace_match_without_losing_labels() {
        assert!(matches_query(
            "terminal",
            Section::Terminal,
            "Font size",
            "Pixels"
        ));
        assert!(matches_query(
            "  font   size ",
            Section::Terminal,
            "Font size",
            "Pixels"
        ));
        assert!(matches_query(
            "",
            Section::System,
            "Sample interval",
            "Seconds"
        ));
        assert!(!matches_query(
            "unrelated",
            Section::System,
            "Sample interval",
            "Seconds"
        ));
    }
}
