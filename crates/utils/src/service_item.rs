#[derive(Clone, Debug)]
pub struct ServiceItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}

impl ServiceItem {
    pub fn is_running(&self) -> bool {
        if self.status.contains("0") || !self.status.to_ascii_lowercase().contains("inactive") {
            return true;
        }
        false
    }
}
