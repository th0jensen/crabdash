//! Process filtering and ordering keep sampled identities intact.
use crate::{
    app::Crabdash,
    components::table::{Direction, Search, Sort},
};
use gpui::{Context, ScrollHandle, UniformListScrollHandle};
use machines::resources::ProcessUsage;
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Column {
    Pid,
    Name,
    Cpu,
    Memory,
}

pub(crate) struct State {
    pub sort: Sort<Column>,
    pub search: Search,
    pub scroll: ScrollHandle,
    pub list_scroll: UniformListScrollHandle,
}
impl State {
    pub(crate) fn new(cx: &mut Context<Crabdash>) -> Self {
        let list_scroll = UniformListScrollHandle::new();
        let scroll = list_scroll.0.borrow().base_handle.clone();
        Self {
            sort: Sort {
                column: Column::Cpu,
                direction: Direction::Descending,
            },
            search: Search::new(
                "Filter processes…",
                |app| {
                    app.system
                        .processes
                        .as_ref()
                        .map(|state| state.scroll.clone())
                        .unwrap_or_else(ScrollHandle::new)
                },
                cx,
            ),
            scroll,
            list_scroll,
        }
    }
    pub(crate) fn visible<'a>(
        &self,
        rows: &'a [ProcessUsage],
        query: &str,
    ) -> Vec<&'a ProcessUsage> {
        visible(rows, self.sort, query)
    }
}
fn optional_order<T: PartialOrd>(a: Option<T>, b: Option<T>, sort: Sort<Column>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => sort.order(a.partial_cmp(&b).unwrap_or(Ordering::Equal)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
fn visible<'a>(rows: &'a [ProcessUsage], sort: Sort<Column>, query: &str) -> Vec<&'a ProcessUsage> {
    let query = query.trim().to_lowercase();
    let mut visible: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.pid.to_string().contains(&query)
                || row.name.to_lowercase().contains(&query)
                || row
                    .user
                    .as_ref()
                    .is_some_and(|user| user.to_lowercase().contains(&query))
        })
        .collect();
    visible.sort_by(|a, b| {
        match sort.column {
            Column::Pid => sort.order(a.pid.cmp(&b.pid)),
            Column::Name => sort.order(a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            Column::Cpu => optional_order(
                a.cpu_percent.filter(|value| value.is_finite()),
                b.cpu_percent.filter(|value| value.is_finite()),
                sort,
            ),
            Column::Memory => optional_order(a.memory_bytes, b.memory_bytes, sort),
        }
        .then_with(|| a.pid.cmp(&b.pid))
        .then_with(|| a.start_id.cmp(&b.start_id))
    });
    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    fn process(pid: u32, name: &str, cpu: Option<f64>) -> ProcessUsage {
        ProcessUsage {
            pid,
            start_id: format!("start-{pid}"),
            name: name.into(),
            user: Some("alice".into()),
            memory_bytes: Some(u64::from(pid) * 1024),
            cpu_percent: cpu,
        }
    }
    #[test]
    fn filtering_and_sorting_keep_unknown_values_last_and_identity_stable() {
        let rows = [
            process(3, "Zulu", None),
            process(2, "alpha", Some(30.0)),
            process(1, "Alpha", Some(10.0)),
            process(4, "NaN", Some(f64::NAN)),
        ];
        let mut sort = Sort {
            column: Column::Cpu,
            direction: Direction::Descending,
        };
        assert_eq!(
            visible(&rows, sort, "")
                .iter()
                .map(|row| row.pid)
                .collect::<Vec<_>>(),
            [2, 1, 3, 4]
        );
        sort.select(Column::Cpu);
        assert_eq!(
            visible(&rows, sort, "")
                .iter()
                .map(|row| row.pid)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        sort.select(Column::Memory);
        assert_eq!(
            visible(&rows, sort, "ALICE")
                .iter()
                .map(|row| row.pid)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        sort.select(Column::Name);
        assert_eq!(
            visible(&rows, sort, "alp")
                .iter()
                .map(|row| row.pid)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(visible(&rows, sort, "3")[0].start_id, "start-3");
    }
}
