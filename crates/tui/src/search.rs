//! Search and filters for every section: a query matched against each row's
//! text, and the filters or sort orders a section offers as chips.
use crate::section::Section;

#[derive(Clone, Debug, Default)]
pub struct Search {
    pub query: String,
    /// The chosen chip in `chips(section)`.
    pub chip: usize,
}

/// The chips a section offers: filters for Docker and Services, sort orders
/// for System's processes. The first is the default.
pub fn chips(section: Section) -> &'static [&'static str] {
    match section {
        Section::Docker => &["all", "running", "stopped"],
        Section::Disks => &["all", "healthy", "other"],
        Section::Services => &["all", "active", "failed"],
        Section::System => &["cpu", "memory", "name"],
    }
}

/// What the search field says while empty.
pub fn placeholder(section: Section) -> &'static str {
    match section {
        Section::Docker => "search containers",
        Section::Disks => "search disks and mounts",
        Section::Services => "search services",
        Section::System => "search processes",
    }
}

/// Whether every word of `query` appears in one of `fields`, ignoring case.
/// An empty query matches everything.
pub fn matches(query: &str, fields: &[&str]) -> bool {
    let haystack = fields.join("\n").to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_must_match_some_field_in_any_case() {
        assert!(matches("", &["anything"]));
        assert!(matches("  ", &[]));
        assert!(matches("Web run", &["web-1", "running"]));
        assert!(!matches("web exited", &["web-1", "running"]));
        // Words don't join across fields.
        assert!(!matches("1run", &["web-1", "running"]));
    }
}
