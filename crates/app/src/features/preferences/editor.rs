use crate::{
    app::Crabdash,
    components::{common::button, style, text_field::TextField},
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            Self::TabWidth => "Minimum tab width",
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
            Self::TabWidth => "Pixels · 120–240; equal widths expand to fit titles and shortcuts",
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
    pub draft: Preferences,
    fields: Inputs,
    pub busy: bool,
    pub error: Option<String>,
    pub mutation_error: Option<String>,
    pub scroll: ScrollHandle,
    pub picker: Option<Field>,
}
impl Editor {
    pub fn new(settings: &Preferences, cx: &mut Context<Crabdash>) -> Self {
        let fields = Inputs::new(settings, cx);
        Self {
            section: Section::General,
            draft: settings.clone(),
            fields,
            busy: false,
            error: None,
            mutation_error: None,
            scroll: ScrollHandle::new(),
            picker: None,
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

fn field_row(editor: &Editor, field: Field, cx: &mut Context<Crabdash>) -> Div {
    let selectable = matches!(
        field,
        Field::TerminalFont | Field::InterfaceFont | Field::Terminfo
    );
    let mut control = div()
        .w(gpui::rems(230.0 / 16.0))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.0))
        .child(div().flex_1().min_w_0().child(editor.input(field)));
    if selectable {
        control = control.child(
            button(
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
            })),
        );
    }
    let mut result = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(row(field.label(), field.hint()).child(control));
    if editor.picker == Some(field) {
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
        result = result.child(
            div()
                .id(SharedString::from(format!("choices-{field:?}")))
                .max_h(px(160.0))
                .overflow_y_scroll()
                .border_1()
                .border_color(rgb(0x383838))
                .bg(rgb(0x181818))
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
                        .hover(|this| this.bg(rgb(0x333333)))
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
fn row(label: &'static str, hint: &'static str) -> Div {
    div().flex().items_center().gap(px(20.0)).child(
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
                    .text_color(rgb(0x929292))
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
    row(label, hint).child(
        div()
            .id(id)
            .w(px(38.0))
            .h(px(22.0))
            .flex_none()
            .px(px(3.0))
            .rounded_full()
            .flex()
            .items_center()
            .bg(if enabled {
                rgb(0x4D8CEC)
            } else {
                rgb(0x505050)
            })
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
    )
}

pub fn render(app: &Crabdash, window: &Window, cx: &mut Context<Crabdash>) -> impl IntoElement {
    let editor = &app.preference_editor;
    let busy = editor.busy || app.startup_busy || super::mutation::is_busy(cx);
    let mut body = div().flex().flex_col().gap(px(22.0));
    match editor.section {
        Section::General => {
            if crate::desktop::startup::SUPPORTED && crate::desktop::tray::SUPPORTED {
                body = body
                    .child(toggle_row(
                        "start-at-login-toggle",
                        "Start at login",
                        "Open Crabdash at desktop login; saved immediately",
                        app.login_startup.enabled,
                        busy,
                        Crabdash::toggle_login_startup,
                        cx,
                    ))
                    .child(toggle_row(
                        "start-minimised-toggle",
                        "Start minimised",
                        "Open in the background on the next launch",
                        editor.draft.start_minimised,
                        busy,
                        |this, _| this.preference_editor.draft.start_minimised ^= true,
                        cx,
                    ))
                    .child(toggle_row(
                        "close-to-tray-toggle",
                        "Keep running when closed",
                        "Keep sessions alive when a system tray is available; Quit exits",
                        editor.draft.close_to_tray,
                        busy,
                        |this, _| this.preference_editor.draft.close_to_tray ^= true,
                        cx,
                    ));
            }
            body = body.child(toggle_row("auto-refresh-toggle", "Automatic refresh", "Update machine status and tables", editor.draft.auto_refresh, busy, |this, _| this.preference_editor.draft.auto_refresh ^= true, cx))
                .child(field_row(editor, Field::Refresh, cx)).child(field_row(editor, Field::LogLines, cx))
                .child(div().text_size(gpui::rems(style::META / 16.0)).text_color(rgb(0x929292)).child("Errors stay visible until dismissed. Refresh manually with the refresh button or shortcut."));
        }
        Section::System => {
            body=body.child(field_row(editor,Field::SystemRefresh,cx))
                .child(div().text_size(gpui::rems(style::META / 16.0)).text_color(rgb(0x929292))
                    .child("Resources update while System is visible, independently of automatic table refresh. Longer intervals reduce sampling and rendering work."));
        }
        Section::Terminal => {
            for field in [
                Field::TerminalFont,
                Field::TerminalSize,
                Field::LineHeight,
                Field::Terminfo,
                Field::Scrollback,
                Field::TerminalRows,
            ] {
                body = body.child(field_row(editor, field, cx));
            }
            body = body.child(toggle_row("true-color-toggle", "Advertise true colour", "Set COLORTERM=truecolor for new sessions when the host permits", editor.draft.true_color, busy, |this, _| this.preference_editor.draft.true_color ^= true, cx))
                .child(div().text_size(gpui::rems(style::META / 16.0)).text_color(rgb(0x929292)).child("Fonts apply to terminal and logs immediately. Terminal type, colour advertisement, and scrollback apply to new sessions. Use xterm for older hosts; xterm-ghostty requires its terminfo on the host."));
        }
        Section::Interface => {
            for field in [
                Field::InterfaceFont,
                Field::InterfaceSize,
                Field::SidebarWidth,
                Field::TabWidth,
            ] {
                body = body.child(field_row(editor, field, cx));
            }
            body = body.child(toggle_row(
                "always-show-shortcuts-toggle",
                "Always show shortcuts",
                "Keep shortcut hints visible; Alt also shows every menu shortcut",
                editor.draft.always_show_shortcuts,
                busy,
                |this, _| this.preference_editor.draft.always_show_shortcuts ^= true,
                cx,
            ));
        }
    }
    let errors = visible_errors(
        editor.error.as_deref(),
        app.login_startup.error.as_deref(),
        editor.mutation_error.as_deref(),
        super::mutation::is_busy(cx),
    );
    div()
        .absolute()
        .inset_0()
        .size_full()
        .bg(rgba(0x00000099))
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .w(px(680.0))
                .max_w_full()
                .h(px(
                    (f32::from(window.viewport_size().height) - 80.0).clamp(240.0, 600.0)
                ))
                .p(px(24.0))
                .rounded(px(style::RADIUS))
                .bg(rgb(0x202020))
                .border_1()
                .border_color(rgb(0x383838))
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(
                    div()
                        .text_size(gpui::rems(16.0 / 16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Preferences"),
                )
                .child(
                    div().flex().gap(px(6.0)).children(
                        [
                            (Section::General, "General"),
                            (Section::Terminal, "Terminal"),
                            (Section::Interface, "Interface"),
                            (Section::System, "System"),
                        ]
                        .into_iter()
                        .map(|(section, label)| {
                            button(
                                SharedString::from(format!("preferences-{label}")),
                                None::<lucide_icons::Icon>,
                                Some(label),
                                editor.section == section,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.preference_editor.section = section;
                                    this.preference_editor
                                        .scroll
                                        .set_offset(point(px(0.0), px(0.0)));
                                    window.focus(&this.focus_handle);
                                    cx.notify();
                                },
                            ))
                        }),
                    ),
                )
                .child(
                    div()
                        .id("preferences-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&editor.scroll)
                        .pr(px(6.0))
                        .child(body),
                )
                .children(errors.into_iter().map(|error| {
                    div()
                        .text_size(gpui::rems(style::META / 16.0))
                        .text_color(rgb(0xFF9F99))
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
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    if !this.preference_editor.busy {
                                        this.preference_editor =
                                            Editor::new(&Preferences::default(), cx);
                                        window.focus(&this.focus_handle);
                                        cx.notify();
                                    }
                                },
                            )),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .child(
                                    button(
                                        "cancel-preferences",
                                        None::<lucide_icons::Icon>,
                                        Some("Cancel"),
                                        false,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, window, cx| {
                                            if !this.preference_editor.busy {
                                                this.preferences_open = false;
                                                window.focus(&this.focus_handle);
                                                cx.notify();
                                            }
                                        },
                                    )),
                                )
                                .child(
                                    button(
                                        "save-preferences",
                                        None::<lucide_icons::Icon>,
                                        Some(if busy { "Saving…" } else { "Apply" }),
                                        true,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, window, cx| this.apply_preferences(window, cx),
                                    )),
                                ),
                        ),
                ),
        )
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
