use crate::app::Crabdash;
use crate::components::terminal_input::{
    TerminalBackspace, TerminalBytes, TerminalClear, TerminalDelete, TerminalDown, TerminalEnd,
    TerminalEnter, TerminalEof, TerminalEscape, TerminalHistoryBottom, TerminalHistoryDown,
    TerminalHistoryTop, TerminalHistoryUp, TerminalHome, TerminalInterrupt, TerminalLeft,
    TerminalPageDown, TerminalPageUp, TerminalRight, TerminalShiftTab, TerminalSuspend,
    TerminalTab, TerminalUp,
};
use crate::components::text_field::{
    FieldBackspace, FieldCopy, FieldCut, FieldDelete, FieldEnd, FieldHome, FieldLeft, FieldPaste,
    FieldRight, FieldSelectAll, FieldSelectLeft, FieldSelectRight, FieldTab, FieldTabPrev,
};
use gpui::*;

use crate::{
    CloseWindow, DismissModal, MinimizeWindow, OpenAddMachine, OpenPreferences, RefreshServices,
    SubmitModal, ToggleFullScreen, ToggleSidebar, ToggleTerminal,
};
impl Crabdash {
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-1", "ctrl-1"),
                crate::ShowDocker,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-2", "ctrl-2"),
                crate::ShowDisks,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-3", "ctrl-3"),
                crate::ShowServices,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-4", "ctrl-4"),
                crate::ShowSystem,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-,", "ctrl-,"),
                OpenPreferences,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-s", "ctrl-b"),
                ToggleSidebar,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-n", "ctrl-n"),
                OpenAddMachine,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-r", "ctrl-r"),
                RefreshServices,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-j", "ctrl-j"),
                ToggleTerminal,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-w", "ctrl-w"),
                CloseWindow,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-m", "ctrl-m"),
                MinimizeWindow,
                None,
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("ctrl-cmd-f", "f11"),
                ToggleFullScreen,
                None,
            ),
            KeyBinding::new("escape", DismissModal, None),
            KeyBinding::new("escape", DismissModal, Some("CrabdashTextField")),
            KeyBinding::new("enter", SubmitModal, Some("CrabdashTextField")),
            KeyBinding::new("return", SubmitModal, Some("CrabdashTextField")),
            KeyBinding::new("backspace", FieldBackspace, Some("CrabdashTextField")),
            KeyBinding::new("delete", FieldDelete, Some("CrabdashTextField")),
            KeyBinding::new("left", FieldLeft, Some("CrabdashTextField")),
            KeyBinding::new("right", FieldRight, Some("CrabdashTextField")),
            KeyBinding::new("shift-left", FieldSelectLeft, Some("CrabdashTextField")),
            KeyBinding::new("shift-right", FieldSelectRight, Some("CrabdashTextField")),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-a", "ctrl-a"),
                FieldSelectAll,
                Some("CrabdashTextField"),
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-v", "ctrl-v"),
                FieldPaste,
                Some("CrabdashTextField"),
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-c", "ctrl-c"),
                FieldCopy,
                Some("CrabdashTextField"),
            ),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-x", "ctrl-x"),
                FieldCut,
                Some("CrabdashTextField"),
            ),
            KeyBinding::new("home", FieldHome, Some("CrabdashTextField")),
            KeyBinding::new("end", FieldEnd, Some("CrabdashTextField")),
            KeyBinding::new("tab", FieldTab, Some("CrabdashTextField")),
            KeyBinding::new("shift-tab", FieldTabPrev, Some("CrabdashTextField")),
            KeyBinding::new(
                "backspace",
                TerminalBackspace,
                Some("CrabdashTerminalInput"),
            ),
            KeyBinding::new("delete", TerminalDelete, Some("CrabdashTerminalInput")),
            KeyBinding::new("enter", TerminalEnter, Some("CrabdashTerminalInput")),
            KeyBinding::new("return", TerminalEnter, Some("CrabdashTerminalInput")),
            KeyBinding::new("escape", TerminalEscape, Some("CrabdashTerminalInput")),
            KeyBinding::new("tab", TerminalTab, Some("CrabdashTerminalInput")),
            KeyBinding::new("shift-tab", TerminalShiftTab, Some("CrabdashTerminalInput")),
            KeyBinding::new("up", TerminalUp, Some("CrabdashTerminalInput")),
            KeyBinding::new("down", TerminalDown, Some("CrabdashTerminalInput")),
            KeyBinding::new("left", TerminalLeft, Some("CrabdashTerminalInput")),
            KeyBinding::new("right", TerminalRight, Some("CrabdashTerminalInput")),
            KeyBinding::new("home", TerminalHome, Some("CrabdashTerminalInput")),
            KeyBinding::new("end", TerminalEnd, Some("CrabdashTerminalInput")),
            KeyBinding::new("pageup", TerminalPageUp, Some("CrabdashTerminalInput")),
            KeyBinding::new("pagedown", TerminalPageDown, Some("CrabdashTerminalInput")),
            KeyBinding::new("ctrl-c", TerminalInterrupt, Some("CrabdashTerminalInput")),
            KeyBinding::new("ctrl-d", TerminalEof, Some("CrabdashTerminalInput")),
            KeyBinding::new("ctrl-z", TerminalSuspend, Some("CrabdashTerminalInput")),
            KeyBinding::new("ctrl-l", TerminalClear, Some("CrabdashTerminalInput")),
            KeyBinding::new(
                crate::desktop::menus::shortcut("cmd-v", "ctrl-shift-v"),
                FieldPaste,
                Some("CrabdashTerminalInput"),
            ),
        ]);
        cx.bind_keys([
            KeyBinding::new(
                "shift-pageup",
                TerminalHistoryUp,
                Some("CrabdashTerminalInput"),
            ),
            KeyBinding::new(
                "shift-pagedown",
                TerminalHistoryDown,
                Some("CrabdashTerminalInput"),
            ),
            KeyBinding::new(
                "ctrl-shift-home",
                TerminalHistoryTop,
                Some("CrabdashTerminalInput"),
            ),
            KeyBinding::new(
                "ctrl-shift-end",
                TerminalHistoryBottom,
                Some("CrabdashTerminalInput"),
            ),
        ]);
        // Shell editing shortcuts take priority while the terminal has focus.
        // Ctrl+J remains Crabdash's terminal toggle.
        for letter in b'a'..=b'z' {
            if [b'c', b'd', b'j', b'l', b'z'].contains(&letter) {
                continue;
            }
            cx.bind_keys([KeyBinding::new(
                &format!("ctrl-{}", char::from(letter)),
                TerminalBytes {
                    bytes: char::from(letter - b'a' + 1).to_string(),
                },
                Some("CrabdashTerminalInput"),
            )]);
        }
        for (binding, bytes) in [
            ("alt-left", "\x1bb"),
            ("alt-right", "\x1bf"),
            ("ctrl-left", "\x1b[1;5D"),
            ("ctrl-right", "\x1b[1;5C"),
            ("alt-backspace", "\x1b\x7f"),
            ("ctrl-backspace", "\x17"),
            ("f1", "\x1bOP"),
            ("f2", "\x1bOQ"),
            ("f3", "\x1bOR"),
            ("f4", "\x1bOS"),
            ("f5", "\x1b[15~"),
            ("f6", "\x1b[17~"),
            ("f7", "\x1b[18~"),
            ("f8", "\x1b[19~"),
            ("f9", "\x1b[20~"),
            ("f10", "\x1b[21~"),
            ("f11", "\x1b[23~"),
            ("f12", "\x1b[24~"),
        ] {
            cx.bind_keys([KeyBinding::new(
                binding,
                TerminalBytes {
                    bytes: bytes.into(),
                },
                Some("CrabdashTerminalInput"),
            )]);
        }
        crate::features::terminal::bind_keys(cx);
    }
}
