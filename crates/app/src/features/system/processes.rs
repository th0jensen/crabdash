//! Process filtering and ordering keep sampled identities intact.
mod cache;
mod view;
use crate::{
    app::Crabdash,
    components::table::{Direction, Search, Sort},
};
use gpui::{Context, ScrollHandle, UniformListScrollHandle};
use machines::resources::{ProcessUsage, ResourceUsage};
use std::{cell::RefCell, cmp::Ordering, rc::Rc};
use uuid::Uuid;
pub(super) use view::render;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Column {
    Pid,
    Name,
    User,
    Cpu,
    Memory,
}

pub(crate) struct State {
    pub sort: Sort<Column>,
    pub search: Search,
    pub scroll: ScrollHandle,
    pub list_scroll: UniformListScrollHandle,
    cache: RefCell<Option<Rc<cache::Prepared>>>,
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
            cache: RefCell::new(None),
        }
    }
    fn prepare(
        &self,
        machine: Uuid,
        source: &Rc<ResourceUsage>,
        query: &str,
    ) -> Rc<cache::Prepared> {
        cache::prepare(&self.cache, machine, source, self.sort, query)
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
fn user_name(row: &ProcessUsage) -> Option<&str> {
    row.user.as_deref().filter(|user| !user.trim().is_empty())
}
fn visible_indices(rows: &[ProcessUsage], sort: Sort<Column>, query: &str) -> Vec<usize> {
    let mut visible: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            query.is_empty()
                || row.pid.to_string().contains(query)
                || row.name.to_lowercase().contains(query)
                || row
                    .user
                    .as_ref()
                    .is_some_and(|user| user.to_lowercase().contains(query))
        })
        .map(|(index, _)| index)
        .collect();
    if matches!(sort.column, Column::Name | Column::User) {
        let mut named: Vec<_> = visible
            .into_iter()
            .map(|index| {
                let key = if sort.column == Column::User {
                    user_name(&rows[index]).map(str::to_lowercase)
                } else {
                    Some(rows[index].name.to_lowercase())
                };
                (index, key)
            })
            .collect();
        named.sort_by(|(a, name_a), (b, name_b)| {
            let a = &rows[*a];
            let b = &rows[*b];
            optional_order(name_a.as_deref(), name_b.as_deref(), sort)
                .then_with(|| a.pid.cmp(&b.pid))
                .then_with(|| a.start_id.cmp(&b.start_id))
        });
        return named.into_iter().map(|(index, _)| index).collect();
    }
    visible.sort_by(|a, b| {
        let a = &rows[*a];
        let b = &rows[*b];
        match sort.column {
            Column::Pid => sort.order(a.pid.cmp(&b.pid)),
            Column::Name | Column::User => Ordering::Equal,
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
fn visible<'a>(rows: &'a [ProcessUsage], sort: Sort<Column>, query: &str) -> Vec<&'a ProcessUsage> {
    visible_indices(rows, sort, &query.trim().to_lowercase())
        .into_iter()
        .filter_map(|index| rows.get(index))
        .collect()
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
    fn user_order_is_case_insensitive_with_unknowns_last_in_both_directions() {
        let mut rows: Vec<_> = (1..=7).map(|pid| process(pid, "worker", None)).collect();
        for (row, user) in rows.iter_mut().zip([
            Some("Zulu"),
            None,
            Some("alice"),
            Some("ALICE"),
            Some(""),
            Some("   "),
            Some("bob"),
        ]) {
            row.user = user.map(str::to_string);
        }
        for (direction, expected) in [
            (Direction::Ascending, vec![3, 4, 7, 1, 2, 5, 6]),
            (Direction::Descending, vec![1, 7, 3, 4, 2, 5, 6]),
        ] {
            let sort = Sort {
                column: Column::User,
                direction,
            };
            assert_eq!(
                visible(&rows, sort, "")
                    .iter()
                    .map(|row| row.pid)
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                visible(&rows, sort, " ALICE ")
                    .iter()
                    .map(|row| row.pid)
                    .collect::<Vec<_>>(),
                [3, 4]
            );
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
    #[test]
    fn full_inventory_search_and_sort_include_processes_beyond_first_hundred() {
        let mut rows: Vec<_> = (1..=250)
            .map(|pid| process(pid, "worker", Some(10.0)))
            .collect();
        rows[200] = process(9001, "archive-indexer", Some(0.0));
        rows[200].user = Some("backup-owner".into());
        rows[200].memory_bytes = Some(16 * 1024 * 1024 * 1024);
        rows[240] = process(1500, "busy-worker", Some(99.0));
        let mut sort = Sort {
            column: Column::Cpu,
            direction: Direction::Descending,
        };
        let cpu = visible(&rows, sort, "");
        assert_eq!(cpu.len(), 250);
        assert_eq!(cpu.first().map(|row| row.pid), Some(1500));
        for query in ["ARCHIVE", "9001", "backup-owner"] {
            let matches = visible(&rows, sort, query);
            assert_eq!(matches.len(), 1);
            assert_eq!(matches.first().map(|row| row.pid), Some(9001));
        }
        sort.column = Column::Memory;
        assert_eq!(
            visible(&rows, sort, "").first().map(|row| row.pid),
            Some(9001)
        );
        sort.column = Column::Pid;
        assert_eq!(
            visible(&rows, sort, "").first().map(|row| row.pid),
            Some(9001)
        );
        sort.direction = Direction::Ascending;
        assert_eq!(visible(&rows, sort, "").first().map(|row| row.pid), Some(1));
        sort.column = Column::Name;
        assert_eq!(
            visible(&rows, sort, "").first().map(|row| row.pid),
            Some(9001)
        );
    }
}
