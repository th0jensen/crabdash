//! Sidebar colour policy; platform changes do not alter row geometry.
#[cfg(target_os = "macos")]
pub(super) const BACKGROUND: u32 = crate::components::style::CONTENT;
#[cfg(not(target_os = "macos"))]
pub(super) const BACKGROUND: u32 = 0x1B1B1B;

#[cfg(target_os = "macos")]
pub(crate) use crate::components::contrast::selection_text;
