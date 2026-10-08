//! One prepared process order retains its immutable accepted resource sample.
use super::{Column, visible_indices};
use crate::components::table::{Direction, Sort};
use machines::resources::{ProcessUsage, ResourceUsage};
use std::{cell::RefCell, rc::Rc};
use uuid::Uuid;

pub(super) struct Prepared {
    source: Rc<ResourceUsage>,
    machine: Uuid,
    query: String,
    column: Column,
    direction: Direction,
    pub(super) indices: Vec<usize>,
}

impl Prepared {
    pub(super) fn row(&self, index: usize) -> Option<&ProcessUsage> {
        self.source
            .processes
            .as_ref()?
            .get(*self.indices.get(index)?)
    }
}

pub(super) fn prepare(
    cache: &RefCell<Option<Rc<Prepared>>>,
    machine: Uuid,
    source: &Rc<ResourceUsage>,
    sort: Sort<Column>,
    query: &str,
) -> Rc<Prepared> {
    let query = query.trim().to_lowercase();
    if let Some(prepared) = cache.borrow().as_ref()
        && prepared.machine == machine
        && Rc::ptr_eq(&prepared.source, source)
        && prepared.query == query
        && prepared.column == sort.column
        && prepared.direction == sort.direction
    {
        return Rc::clone(prepared);
    }
    let indices = source
        .processes
        .as_deref()
        .map_or_else(Vec::new, |rows| visible_indices(rows, sort, &query));
    let prepared = Rc::new(Prepared {
        source: Rc::clone(source),
        machine,
        query,
        column: sort.column,
        direction: sort.direction,
        indices,
    });
    *cache.borrow_mut() = Some(Rc::clone(&prepared));
    prepared
}

#[cfg(test)]
mod tests {
    use super::*;
    use machines::resources::MemorySample;

    fn process(
        pid: u32,
        name: &str,
        user: &str,
        cpu: Option<f64>,
        memory: Option<u64>,
    ) -> ProcessUsage {
        ProcessUsage {
            pid,
            start_id: format!("start-{pid}"),
            name: name.into(),
            user: Some(user.into()),
            cpu_percent: cpu,
            memory_bytes: memory,
        }
    }

    fn sample(processes: Option<Vec<ProcessUsage>>) -> Rc<ResourceUsage> {
        Rc::new(ResourceUsage {
            cpu_percent: None,
            cores: vec![],
            logical_cpus: 1,
            memory: MemorySample {
                total_bytes: 1024,
                available_bytes: 512,
                estimated: false,
            },
            swap: None,
            load_average: None,
            uptime_seconds: 10.0,
            process_count: processes.as_ref().map(Vec::len),
            processes,
            processes_truncated: false,
            network: None,
            disks: None,
            gpus: None,
        })
    }

    fn sort(column: Column, direction: Direction) -> Sort<Column> {
        Sort { column, direction }
    }

    fn pids(prepared: &Prepared) -> Vec<u32> {
        (0..prepared.indices.len())
            .filter_map(|index| prepared.row(index).map(|row| row.pid))
            .collect()
    }

    #[test]
    fn unchanged_sample_and_normalized_query_reuse_preparation_without_holding_a_borrow() {
        let cache = RefCell::new(None);
        let machine = Uuid::new_v4();
        let source = sample(Some(vec![process(7, "Worker", "Alice", Some(1.0), None)]));
        let sort = sort(Column::Cpu, Direction::Descending);
        let first = prepare(&cache, machine, &source, sort, "  ALICE ");
        let next = prepare(&cache, machine, &Rc::clone(&source), sort, "alice");
        assert!(Rc::ptr_eq(&first, &next));
        assert_eq!(pids(&next), [7]);
        // Render callbacks can retain preparation without retaining a RefCell lease.
        assert!(cache.borrow_mut().is_some());
    }

    #[test]
    fn changed_query_column_and_direction_each_invalidate_the_single_cache_entry() {
        let cache = RefCell::new(None);
        let machine = Uuid::new_v4();
        let source = sample(Some(vec![
            process(7, "alpha", "alice", Some(1.0), Some(200)),
            process(8, "beta", "alice", Some(2.0), Some(100)),
        ]));
        let cpu = sort(Column::Cpu, Direction::Descending);
        let all = prepare(&cache, machine, &source, cpu, "");
        let filtered = prepare(&cache, machine, &source, cpu, "beta");
        assert!(!Rc::ptr_eq(&all, &filtered));
        assert_eq!(pids(&filtered), [8]);
        let unfiltered = prepare(&cache, machine, &source, cpu, "");
        let memory = prepare(
            &cache,
            machine,
            &source,
            sort(Column::Memory, Direction::Descending),
            "",
        );
        assert!(!Rc::ptr_eq(&unfiltered, &memory));
        assert_eq!(pids(&memory), [7, 8]);
        let ascending = prepare(
            &cache,
            machine,
            &source,
            sort(Column::Memory, Direction::Ascending),
            "",
        );
        assert!(!Rc::ptr_eq(&memory, &ascending));
        assert_eq!(pids(&ascending), [8, 7]);
    }

    #[test]
    fn new_sample_updates_same_pid_fields_while_retained_callback_reads_its_original_sample()
    -> anyhow::Result<()> {
        let cache = RefCell::new(None);
        let machine = Uuid::new_v4();
        let cpu = sort(Column::Cpu, Direction::Descending);
        let old_source = sample(Some(vec![
            process(7, "old-worker", "alice", Some(1.0), Some(100)),
            process(8, "other", "alice", Some(10.0), Some(200)),
        ]));
        let old_callback = prepare(&cache, machine, &old_source, cpu, "");
        let new_source = sample(Some(vec![
            process(7, "new-worker", "bob", Some(20.0), Some(999)),
            process(8, "other", "alice", Some(10.0), Some(200)),
        ]));
        let current = prepare(&cache, machine, &new_source, cpu, "");
        assert!(!Rc::ptr_eq(&old_callback, &current));
        assert_eq!(pids(&current), [7, 8]);
        let row = current
            .row(0)
            .ok_or_else(|| anyhow::anyhow!("Missing current row"))?;
        assert_eq!(
            (
                &*row.name,
                row.user.as_deref(),
                row.cpu_percent,
                row.memory_bytes
            ),
            ("new-worker", Some("bob"), Some(20.0), Some(999))
        );
        assert_eq!(pids(&old_callback), [8, 7]);
        let old = old_callback
            .row(1)
            .ok_or_else(|| anyhow::anyhow!("Missing retained row"))?;
        assert_eq!(
            (
                &*old.name,
                old.user.as_deref(),
                old.cpu_percent,
                old.memory_bytes
            ),
            ("old-worker", Some("alice"), Some(1.0), Some(100))
        );
        assert_eq!(
            pids(&prepare(&cache, machine, &new_source, cpu, "bob")),
            [7]
        );
        assert!(pids(&prepare(&cache, machine, &new_source, cpu, "old-worker")).is_empty());
        assert_eq!(
            pids(&prepare(
                &cache,
                machine,
                &new_source,
                sort(Column::Memory, Direction::Descending),
                ""
            )),
            [7, 8]
        );
        Ok(())
    }

    #[test]
    fn switching_machines_and_back_never_mixes_retained_sources() {
        let cache = RefCell::new(None);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let source_a = sample(Some(vec![process(7, "A", "alice", None, None)]));
        let source_b = sample(Some(vec![process(7, "B", "bob", None, None)]));
        let sort = sort(Column::Name, Direction::Ascending);
        let first_a = prepare(&cache, a, &source_a, sort, "");
        let only_machine_changed = prepare(&cache, b, &source_a, sort, "");
        assert!(!Rc::ptr_eq(&first_a, &only_machine_changed));
        let current_b = prepare(&cache, b, &source_b, sort, "");
        let next_a = prepare(&cache, a, &source_a, sort, "");
        assert!(!Rc::ptr_eq(&first_a, &next_a));
        assert_eq!(next_a.row(0).map(|row| row.name.as_str()), Some("A"));
        assert_eq!(current_b.row(0).map(|row| row.name.as_str()), Some("B"));
        assert_eq!(first_a.row(0).map(|row| row.name.as_str()), Some("A"));
    }

    #[test]
    fn missing_and_empty_process_inventory_prepare_safely() {
        let cache = RefCell::new(None);
        let machine = Uuid::new_v4();
        let sort = sort(Column::Cpu, Direction::Descending);
        for source in [sample(None), sample(Some(vec![]))] {
            let prepared = prepare(&cache, machine, &source, sort, "worker");
            assert!(prepared.indices.is_empty());
            assert!(prepared.row(0).is_none());
            let repeat = prepare(&cache, machine, &source, sort, "worker");
            assert!(Rc::ptr_eq(&prepared, &repeat));
        }
    }

    #[test]
    fn ordering_keeps_unknowns_last_and_same_pid_ties_use_start_identity() {
        let cache = RefCell::new(None);
        let mut later = process(7, "same", "alice", Some(1.0), Some(100));
        later.start_id = "later".into();
        let mut earlier = process(7, "same", "alice", Some(1.0), Some(100));
        earlier.start_id = "earlier".into();
        let source = sample(Some(vec![
            process(1, "unknown", "alice", Some(f64::NAN), None),
            later,
            earlier,
        ]));
        for column in [Column::Cpu, Column::Memory] {
            for direction in [Direction::Ascending, Direction::Descending] {
                let prepared = prepare(&cache, Uuid::nil(), &source, sort(column, direction), "");
                assert_eq!(pids(&prepared), [7, 7, 1]);
                assert_eq!(
                    prepared.row(0).map(|row| row.start_id.as_str()),
                    Some("earlier")
                );
                assert_eq!(
                    prepared.row(1).map(|row| row.start_id.as_str()),
                    Some("later")
                );
            }
        }
    }

    #[test]
    fn owner_updates_reprepare_without_changing_retained_identity() -> anyhow::Result<()> {
        let cache = RefCell::new(None);
        let machine = Uuid::new_v4();
        let mut earlier = process(7, "worker", "ALICE", None, None);
        earlier.start_id = "earlier".into();
        let mut later = earlier.clone();
        later.start_id = "later".into();
        let original = sample(Some(vec![later.clone(), earlier]));
        let user_sort = sort(Column::User, Direction::Descending);
        let retained = prepare(&cache, machine, &original, user_sort, "");
        assert_eq!(
            retained.row(0).map(|row| row.start_id.as_str()),
            Some("earlier")
        );
        assert_eq!(
            retained.row(1).map(|row| row.start_id.as_str()),
            Some("later")
        );
        later.user = Some("bob".into());
        let replacement = sample(Some(vec![later]));
        let current = prepare(&cache, machine, &replacement, user_sort, "");
        assert!(!Rc::ptr_eq(&retained, &current));
        let row = current
            .row(0)
            .ok_or_else(|| anyhow::anyhow!("Current process"))?;
        assert_eq!(
            (row.pid, row.start_id.as_str(), row.user.as_deref()),
            (7, "later", Some("bob"))
        );
        assert_eq!(
            retained.row(0).and_then(|row| row.user.as_deref()),
            Some("ALICE")
        );
        assert_eq!(
            pids(&prepare(&cache, machine, &replacement, user_sort, " BOB ")),
            [7]
        );
        assert!(pids(&prepare(&cache, machine, &replacement, user_sort, "alice")).is_empty());
        Ok(())
    }
}
