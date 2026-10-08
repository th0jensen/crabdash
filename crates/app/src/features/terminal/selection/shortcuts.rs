//! Terminal shortcuts follow the desktop's shell-interrupt convention.
#[cfg(target_os = "windows")]
pub(super) fn interrupt_copies(has_selection: bool) -> bool {
    has_selection
}

#[cfg(not(target_os = "windows"))]
pub(super) fn interrupt_copies(_: bool) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub(super) fn copy_keys() -> &'static [&'static str] {
    &["cmd-c"]
}

#[cfg(not(target_os = "macos"))]
pub(super) fn copy_keys() -> &'static [&'static str] {
    &["ctrl-shift-c", "ctrl-insert"]
}

#[cfg(test)]
mod tests {
    use super::{copy_keys, interrupt_copies};

    #[test]
    fn interrupt_routing_uses_current_selection_and_platform_convention() {
        assert!(!interrupt_copies(false));
        #[cfg(target_os = "windows")]
        assert!(interrupt_copies(true));
        #[cfg(not(target_os = "windows"))]
        assert!(!interrupt_copies(true));
        // Clearing/pruning the selection before dispatch restores the interrupt route.
        assert!(!interrupt_copies(false));
    }

    #[test]
    fn copy_shortcuts_keep_control_c_available_to_the_shell() {
        #[cfg(target_os = "macos")]
        assert_eq!(copy_keys(), &["cmd-c"]);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(copy_keys(), &["ctrl-shift-c", "ctrl-insert"]);
        assert!(!copy_keys().contains(&"ctrl-c"));
    }
}
