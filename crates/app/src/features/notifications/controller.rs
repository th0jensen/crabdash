use crate::app::Crabdash;

impl Crabdash {
    pub(crate) fn set_status_error(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into().trim().to_string());
    }

    pub(crate) fn clear_status_message(&mut self) {
        self.status_message = None;
    }
}
