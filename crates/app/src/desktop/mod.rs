//! Integration with the local desktop, independent of the selected SSH machine.
pub(crate) mod about;
pub(crate) mod appearance;
pub(crate) mod menus;
mod runtime;
pub(crate) mod startup;
pub(crate) mod tray;
pub(crate) mod window;
pub use runtime::run;
