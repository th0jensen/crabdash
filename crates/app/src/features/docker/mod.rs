//! Docker feature.
mod controller;
mod view;
pub(crate) use view::*;
pub(crate) mod remove_modal;
mod run;
pub(crate) mod run_modal;
pub(crate) use run::DockerRunConfig;

use uuid::Uuid;
#[derive(Clone)]
pub(crate) struct DockerRemoval {
    pub machine_uuid: Uuid,
    pub id: String,
    pub name: String,
    pub machine_name: String,
    pub active: bool,
    pub force: bool,
}

pub(crate) mod table;
