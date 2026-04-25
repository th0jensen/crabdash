#[derive(Clone, Debug, Default)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}

impl Container {
    pub fn is_running_status(&self) -> bool {
        let normalized = self.status.to_ascii_lowercase();
        normalized.contains("running") || normalized.contains("healthy")
    }
}
