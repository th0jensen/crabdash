//! On-demand, read-only container configuration snapshots.
mod controller;
mod state;
mod view;

pub(crate) use state::{Entry, Key, State};
pub(crate) use view::render;
