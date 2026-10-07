//! Machine selection, connection forms, and store updates.
pub(crate) mod add_modal;
mod controller;
pub(crate) mod logos;
mod model;
pub(crate) mod sidebar;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AddMachineAuthMode {
    #[default]
    None,
    Password,
    AuthKey,
}

impl AddMachineAuthMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Password => "Password",
            Self::AuthKey => "Auth Key",
        }
    }
}
