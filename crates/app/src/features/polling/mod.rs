//! Visibility drives table reads; live System sampling has its own schedule.
mod controller;
mod policy;
pub(crate) use policy::{Domains, State, Target};
