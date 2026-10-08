//! Terminal feature.
mod actions;
pub(crate) use actions::{
    CloseSession, FocusDown, FocusLeft, FocusRight, FocusUp, NewSession, NextTab, PreviousTab,
    SplitDown, SplitRight, bind_keys, decorate,
};
mod controller;
mod docking;
mod geometry;
mod grid;
mod header;
pub(crate) mod height;
mod model;
mod panes;
mod rename;
mod selection;
mod sizing;
mod split;
mod view;
pub(crate) use view::*;
mod state;
pub(crate) use state::*;
