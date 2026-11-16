//! The dashboard sections, matching the desktop app's tabs and their order.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Section {
    #[default]
    Docker,
    Disks,
    Services,
    System,
}

impl Section {
    pub const ALL: [Self; 4] = [Self::Docker, Self::Disks, Self::Services, Self::System];

    /// The command-line and Rex action name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Disks => "disks",
            Self::Services => "services",
            Self::System => "system",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Disks => "Disks",
            Self::Services => "Services",
            Self::System => "System",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.name() == name)
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or_default()
    }

    /// The next section, or the previous one with `step = -1`, wrapping around.
    pub fn step(self, step: isize) -> Self {
        let count = Self::ALL.len() as isize;
        Self::ALL[(self.index() as isize + step).rem_euclid(count) as usize]
    }
}
