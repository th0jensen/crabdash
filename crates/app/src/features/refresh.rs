//! Coalesces reads and rejects results superseded by a mutation or removed target.
use std::{collections::HashMap, hash::Hash};
use uuid::Uuid;

pub(crate) struct Requests<K = Uuid> {
    next: u64,
    active: HashMap<K, u64>,
}

impl<K> Default for Requests<K> {
    fn default() -> Self {
        Self {
            next: 0,
            active: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash> Requests<K> {
    pub(crate) fn begin(&mut self, key: K) -> Option<u64> {
        if self.active.contains_key(&key) {
            return None;
        }
        Some(self.restart(key))
    }

    pub(crate) fn restart(&mut self, key: K) -> u64 {
        self.next = self.next.wrapping_add(1);
        self.active.insert(key, self.next);
        self.next
    }

    pub(crate) fn complete(&mut self, key: &K, ticket: u64) -> bool {
        if self.active.get(key) != Some(&ticket) {
            return false;
        }
        self.active.remove(key);
        true
    }

    pub(crate) fn forget(&mut self, key: &K) {
        self.active.remove(key);
    }

    pub(crate) fn forget_where(&mut self, predicate: impl Fn(&K) -> bool) {
        self.active.retain(|key, _| !predicate(key));
    }
}

#[cfg(test)]
mod tests {
    use super::Requests;

    #[test]
    fn overlapping_reads_coalesce_without_blocking_other_targets() {
        let mut requests = Requests::default();
        let Some(first) = requests.begin("fedora") else {
            panic!("first request must start");
        };
        assert!(requests.begin("fedora").is_none());
        let Some(second) = requests.begin("other") else {
            panic!("independent target must start");
        };
        assert!(requests.complete(&"other", second));
        assert!(requests.complete(&"fedora", first));
        assert!(requests.begin("fedora").is_some());
    }

    #[test]
    fn superseded_or_removed_reads_cannot_complete_new_requests() {
        let mut requests = Requests::default();
        let old = requests.restart("fedora");
        let current = requests.restart("fedora");
        assert!(!requests.complete(&"fedora", old));
        assert!(requests.complete(&"fedora", current));
        let removed = requests.restart("fedora");
        requests.forget(&"fedora");
        let recreated = requests.restart("fedora");
        assert!(!requests.complete(&"fedora", removed));
        assert!(requests.complete(&"fedora", recreated));
    }

    #[test]
    fn machine_cleanup_preserves_other_log_requests() {
        let mut requests = Requests::default();
        let first = requests.restart((1, "logs"));
        let other = requests.restart((2, "logs"));
        requests.forget_where(|key| key.0 == 1);
        assert!(!requests.complete(&(1, "logs"), first));
        assert!(requests.complete(&(2, "logs"), other));
    }
}
