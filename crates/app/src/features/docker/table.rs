//! Container filtering and ordering, independent of platform and rendering.
use crate::app::Crabdash;
use crate::components::table::{Search, Sort};
use gpui::Context;
use services::docker::DockerFilter;
use utils::container::Container;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Column {
    Name,
    Id,
    Status,
}
pub(crate) struct State {
    pub filter: DockerFilter,
    pub sort: Sort<Column>,
    pub search: Search,
}
impl State {
    pub fn new(cx: &mut Context<Crabdash>) -> Self {
        Self {
            filter: DockerFilter::Total,
            sort: Sort::new(Column::Name),
            search: Search::new(
                "Filter containers…",
                |app| app.docker_scroll_handle.clone(),
                cx,
            ),
        }
    }
    pub fn visible<'a>(&self, rows: &'a [Container], query: &str) -> Vec<&'a Container> {
        visible(rows, self.filter, self.sort, query)
    }
}
fn visible<'a>(
    rows: &'a [Container],
    filter: DockerFilter,
    sort: Sort<Column>,
    query: &str,
) -> Vec<&'a Container> {
    let mut rows: Vec<_> = rows
        .iter()
        .filter(|c| match filter {
            DockerFilter::Total => true,
            DockerFilter::Running => c.is_running_status(),
            DockerFilter::Paused => c.is_paused(),
            DockerFilter::Stopped => !c.is_active_status(),
        })
        .filter(|c| {
            [c.name.as_str(), c.id.as_str(), c.status.as_str()]
                .iter()
                .any(|v| v.to_lowercase().contains(query))
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
    fn row(id: &str, name: &str, status: &str) -> Container {
        Container {
            id: id.into(),
            name: name.into(),
            status: status.into(),
            ..Default::default()
        }
    }
    #[test]
    fn filters_search_and_sort_preserve_container_identity() {
        let rows = vec![
            row("c", "Zulu", "running"),
            row("b", "alpha", "paused"),
            row("a", "Alpha", "exited"),
        ];
        let mut sort = Sort::new(Column::Name);
        assert_eq!(
            visible(&rows, DockerFilter::Total, sort, "")
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        sort.select(Column::Name);
        assert_eq!(
            visible(&rows, DockerFilter::Total, sort, "")
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["c", "b", "a"]
        );
        assert_eq!(
            visible(&rows, DockerFilter::Paused, sort, "alpha")[0].id,
            "b"
        );
        assert!(visible(&rows, DockerFilter::Running, sort, "alpha").is_empty());
        assert_eq!(visible(&rows, DockerFilter::Stopped, sort, "")[0].id, "a");
        sort.select(Column::Status);
        assert_eq!(visible(&rows, DockerFilter::Total, sort, "")[0].id, "a");
    }
}
