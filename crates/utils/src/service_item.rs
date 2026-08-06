#[derive(Clone, Debug)]
pub struct ServiceItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub description: Option<String>,
    pub load_state: Option<String>,
    pub sub_state: Option<String>,
    pub unit_file_state: Option<String>,
    pub error: Option<String>,
}

impl ServiceItem {
    pub fn status_label(&self) -> &'static str {
        let status = self.status.trim().to_ascii_lowercase();

        if matches!(status.as_str(), "active" | "running") {
            return "Active";
        }

        if matches!(status.as_str(), "failed" | "crashed") {
            return "Failed";
        }

        if matches!(status.as_str(), "inactive" | "dead" | "exited") {
            return "Inactive";
        }

        if self.id.trim().parse::<u32>().is_ok_and(|pid| pid > 0) {
            return "Active";
        }

        if status.parse::<i32>().is_ok_and(|code| code != 0) {
            return "Failed";
        }

        "Inactive"
    }

    pub fn is_running(&self) -> bool {
        self.status_label() == "Active"
    }
}
