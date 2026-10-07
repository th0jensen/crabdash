//! Service table policy shared by Linux systemd and macOS launchd.
use crate::app::Crabdash;
use crate::components::table::{Search, Sort};
use gpui::Context;
use utils::services::ServiceItem;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Column {
    Name,
    Details,
    Status,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Filter {
    All,
    Active,
    Inactive,
    Failed,
}
impl Filter {
    pub fn matches(self, service: &ServiceItem) -> bool {
        match self {
            Self::All => true,
            Self::Active => service.is_running(),
            Self::Inactive => service.status_label() == "Inactive",
            Self::Failed => service.status_label() == "Failed",
        }
    }
}
pub(crate) struct State {
    pub filter: Filter,
    pub sort: Sort<Column>,
    pub search: Search,
    pub(super) list: super::virtual_list::State,
}
impl State {
    pub fn new(cx: &mut Context<Crabdash>) -> Self {
        Self {
            filter: Filter::All,
            sort: Sort::new(Column::Name),
            search: Search::with_reset(
                "Filter services…",
                |app| app.services_table.list.scroll_to_top(),
                cx,
            ),
            list: super::virtual_list::State::new(),
        }
    }
    pub fn visible<'a>(&self, rows: &'a [ServiceItem], query: &str) -> Vec<&'a ServiceItem> {
        visible(rows, self.filter, self.sort, query)
    }
}
pub(super) fn metadata(service: &ServiceItem) -> String {
    let mut metadata = Vec::new();
    if let Some(state) = service
        .load_state
        .as_ref()
        .filter(|state| state.as_str() != "loaded")
    {
        metadata.push(state.clone());
    }
    if let Some(state) = service
        .sub_state
        .as_ref()
        .filter(|state| !matches!(state.as_str(), "running" | "dead" | "failed"))
    {
        metadata.push(state.clone());
    }
    if let Some(state) = &service.unit_file_state {
        metadata.push(state.clone());
    }
    if service.id.trim().parse::<u32>().is_ok_and(|pid| pid > 0) {
        metadata.push(format!("PID {}", service.id));
    }
    metadata.join(" · ")
}
fn visible<'a>(
    rows: &'a [ServiceItem],
    filter: Filter,
    sort: Sort<Column>,
    query: &str,
) -> Vec<&'a ServiceItem> {
    let mut rows: Vec<_> = rows
        .iter()
        .filter(|s| filter.matches(s))
        .filter(|s| {
            [
                s.name.as_str(),
                s.description.as_deref().unwrap_or(""),
                s.status_label(),
                &metadata(s),
            ]
            .iter()
            .any(|v| v.to_lowercase().contains(query))
        })
        .collect();
    rows.sort_by(|a, b| {
        sort.order(
            match sort.column {
                Column::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                Column::Details => metadata(a).to_lowercase().cmp(&metadata(b).to_lowercase()),
                Column::Status => a.status_label().cmp(b.status_label()),
            }
            .then_with(|| a.name.cmp(&b.name)),
        )
    });
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row(name: &str, id: &str, status: &str) -> ServiceItem {
        ServiceItem {
            id: id.into(),
            name: name.into(),
            status: status.into(),
            description: Some("Test daemon".into()),
            load_state: None,
            sub_state: None,
            unit_file_state: None,
            error: None,
        }
    }
    #[test]
    fn filters_normalize_linux_and_macos_states() {
        let rows = vec![
            row("zeta", "42", "0"),
            row("alpha", "-", "7"),
            row("beta", "0", "inactive"),
        ];
        let mut sort = Sort::new(Column::Name);
        assert_eq!(
            visible(&rows, Filter::Active, sort, "daemon")[0].name,
            "zeta"
        );
        assert_eq!(visible(&rows, Filter::Failed, sort, "")[0].name, "alpha");
        assert_eq!(visible(&rows, Filter::Inactive, sort, "")[0].name, "beta");
        assert!(visible(&rows, Filter::Failed, sort, "missing").is_empty());
        sort.select(Column::Name);
        assert_eq!(visible(&rows, Filter::All, sort, "")[0].name, "zeta");
        sort.select(Column::Status);
        assert_eq!(visible(&rows, Filter::All, sort, "")[0].name, "zeta");
        assert_eq!(metadata(&rows[1]), "");
    }
}
