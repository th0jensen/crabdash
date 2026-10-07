//! Disk filtering and ordering preserve the physical device/partition tree.
use crate::app::Crabdash;
use crate::components::table::{Search, Sort};
use gpui::Context;
use utils::disks::{Disk, DiskNode};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Column {
    Name,
    Id,
    Status,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Filter {
    All,
    Mounted,
    Unmounted,
}
impl Filter {
    pub fn matches(self, disk: &Disk) -> bool {
        let mounted = matches!(
            disk.status.to_ascii_lowercase().as_str(),
            "mounted" | "swap"
        );
        match self {
            Self::All => true,
            Self::Mounted => mounted,
            Self::Unmounted => !mounted,
        }
    }
}
pub(crate) struct State {
    pub filter: Filter,
    pub sort: Sort<Column>,
    pub search: Search,
}
impl State {
    pub fn new(cx: &mut Context<Crabdash>) -> Self {
        Self {
            filter: Filter::All,
            sort: Sort::new(Column::Name),
            search: Search::new("Filter disks…", |app| app.disks_scroll_handle.clone(), cx),
        }
    }
    pub fn visible<'a>(&self, rows: &'a [Disk], query: &str) -> Vec<&'a Disk> {
        visible(rows, self.filter, self.sort, query)
    }
}
fn node_matches(node: &DiskNode, query: &str) -> bool {
    [
        node.name.as_str(),
        node.detail.as_deref().unwrap_or(""),
        node.size.as_deref().unwrap_or(""),
    ]
    .iter()
    .any(|v| v.to_lowercase().contains(query))
        || node.nodes.iter().any(|child| node_matches(child, query))
}
fn visible<'a>(rows: &'a [Disk], filter: Filter, sort: Sort<Column>, query: &str) -> Vec<&'a Disk> {
    let mut rows: Vec<_> = rows
        .iter()
        .filter(|d| filter.matches(d))
        .filter(|d| {
            [
                d.name.as_str(),
                d.id.as_str(),
                d.status.as_str(),
                d.detail.as_deref().unwrap_or(""),
                d.size.as_deref().unwrap_or(""),
            ]
            .iter()
            .any(|v| v.to_lowercase().contains(query))
                || d.nodes.iter().any(|node| node_matches(node, query))
        })
        .collect();
    rows.sort_by(|a, b| {
        sort.order(
            match sort.column {
                Column::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                Column::Id => a.id.cmp(&b.id),
                Column::Status => a.status.cmp(&b.status),
            }
            .then_with(|| a.id.cmp(&b.id)),
        )
    });
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn searching_partitions_keeps_parent_tree_and_mount_filters() {
        let rows = vec![
            Disk {
                id: "b".into(),
                name: "Zulu".into(),
                status: "mounted".into(),
                nodes: vec![DiskNode {
                    name: "partition".into(),
                    detail: Some("/home".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Disk {
                id: "a".into(),
                name: "Alpha".into(),
                status: "healthy".into(),
                ..Default::default()
            },
        ];
        let mut sort = Sort::new(Column::Name);
        assert_eq!(visible(&rows, Filter::All, sort, "")[0].id, "a");
        sort.select(Column::Name);
        assert_eq!(visible(&rows, Filter::All, sort, "")[0].id, "b");
        let found = visible(&rows, Filter::Mounted, sort, "/home");
        assert_eq!(found[0].id, "b");
        assert_eq!(found[0].nodes[0].name, "partition");
        assert_eq!(visible(&rows, Filter::Unmounted, sort, "")[0].id, "a");
        assert!(visible(&rows, Filter::Unmounted, sort, "/home").is_empty());
    }
}
