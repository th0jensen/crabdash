//! Identifies a modal submission independently of selection and later submissions.
use uuid::Uuid;

#[derive(Default)]
pub(super) struct RunRequest {
    active: Option<(Uuid, Uuid)>,
}

impl RunRequest {
    pub fn begin(&mut self, machine: Uuid) -> Uuid {
        let ticket = Uuid::new_v4();
        self.active = Some((machine, ticket));
        ticket
    }

    pub fn complete(&mut self, machine: Uuid, ticket: Uuid) -> bool {
        if self.active != Some((machine, ticket)) {
            return false;
        }
        self.active = None;
        true
    }

    pub fn cancel_for(&mut self, machine: Uuid) -> bool {
        if self.active.is_some_and(|(target, _)| target == machine) {
            self.active = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleted_target_completion_cannot_clear_a_new_submission() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut requests = RunRequest::default();
        let old = requests.begin(first);
        assert!(!requests.cancel_for(second));
        assert!(requests.cancel_for(first));
        let current = requests.begin(second);
        assert!(!requests.complete(first, old));
        assert!(requests.complete(second, current));
    }

    #[test]
    fn reopened_same_machine_rejects_prior_completion() {
        let machine = Uuid::new_v4();
        let mut requests = RunRequest::default();
        let old = requests.begin(machine);
        let current = requests.begin(machine);
        assert!(!requests.complete(machine, old));
        assert!(requests.complete(machine, current));
        assert!(!requests.complete(machine, current));
    }
}
