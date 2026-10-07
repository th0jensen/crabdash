#[derive(Clone, Debug, Default)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}

impl Container {
    pub fn is_running_status(&self) -> bool {
        self.status.eq_ignore_ascii_case("running")
    }

    pub fn is_paused(&self) -> bool {
        self.status.eq_ignore_ascii_case("paused")
    }

    pub fn is_active_status(&self) -> bool {
        matches!(
            self.status.to_ascii_lowercase().as_str(),
            "running" | "paused" | "restarting"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_and_restarting_are_active_but_not_running() {
        for state in ["paused", "restarting"] {
            let container = Container {
                status: state.into(),
                ..Default::default()
            };
            assert!(container.is_active_status());
            assert!(!container.is_running_status());
        }
        let container = Container {
            status: "unhealthy".into(),
            ..Default::default()
        };
        assert!(!container.is_running_status());
    }
}

pub fn parse(output: &crate::output::Output) -> Vec<Container> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');

            let id = parts.next()?.to_string();
            let name = parts.next()?.to_string();
            let state = parts.next()?.to_string();

            Some(Container {
                id,
                name,
                status: state,
                error: None,
            })
        })
        .collect()
}
