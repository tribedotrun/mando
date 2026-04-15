//! Dispatch logic — ready→in-progress slot allocation.

use std::collections::HashMap;

use mando_types::task::{ItemStatus, Task};

/// Default resource name when a task has no explicit `resource` field.
///
/// Tasks without a resource are scheduled against the generic `cc` pool
/// (Claude Code). This default is intentional and documented — callers must
/// treat `item.resource.as_deref().unwrap_or(DEFAULT_RESOURCE)` as the single
/// source of truth for resource lookup rather than hard-coding the literal.
pub const DEFAULT_RESOURCE: &str = "cc";

/// Result of a dispatch check for a single item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchDecision {
    /// Spawn a worker for this item.
    Spawn,
    /// No slot available — skip.
    NoSlot,
    /// Item blocked by resource limit.
    ResourceBlocked(String),
    /// Item not dispatchable (wrong status, etc.).
    NotReady,
}

/// Check if a ready item can be dispatched.
///
/// Returns `Spawn` if a slot is available, `NoSlot` otherwise.
pub(crate) fn check_dispatch(
    item: &Task,
    active_workers: usize,
    max_workers: usize,
    resource_limits: &HashMap<String, usize>,
    resource_counts: &HashMap<String, usize>,
) -> DispatchDecision {
    match item.status {
        ItemStatus::Queued | ItemStatus::Rework => {}
        _ => return DispatchDecision::NotReady,
    };

    if active_workers >= max_workers {
        return DispatchDecision::NoSlot;
    }

    // Check resource-specific limits.
    let resource = item.resource.as_deref().unwrap_or(DEFAULT_RESOURCE);
    if let Some(&limit) = resource_limits.get(resource) {
        let current = resource_counts.get(resource).copied().unwrap_or(0);
        if current >= limit {
            return DispatchDecision::ResourceBlocked(resource.to_string());
        }
    }

    DispatchDecision::Spawn
}

/// Count active resources across in-progress items.
/// Planning-mode items are excluded (they don't consume worker/resource slots).
pub(crate) fn count_resources(items: &[Task]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for item in items {
        if item.status == ItemStatus::InProgress && !item.planning {
            let resource = item.resource.as_deref().unwrap_or(DEFAULT_RESOURCE);
            *counts.entry(resource.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

/// Find items eligible for regular worker dispatch, in priority order.
///
/// Items with status `ready` or `rework` are eligible. Planning-mode items
/// are excluded (dispatched separately by `dispatch_planning`).
/// Sorted by: rework first, then by creation order (position in list).
pub(crate) fn dispatchable_items(items: &[Task]) -> Vec<usize> {
    let mut candidates: Vec<(usize, bool)> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        if item.planning {
            continue;
        }
        match item.status {
            ItemStatus::Rework => candidates.push((i, true)),
            ItemStatus::Queued => candidates.push((i, false)),
            _ => {}
        }
    }

    // Rework items first (priority), then ready items.
    candidates.sort_by_key(|&(_, is_rework)| if is_rework { 0 } else { 1 });
    candidates.into_iter().map(|(i, _)| i).collect()
}

/// Find new items that need clarification.
pub(crate) fn new_items(items: &[Task]) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.status == ItemStatus::New)
        .map(|(i, _)| i)
        .collect()
}

/// Find items that need (re-)clarification — both `Clarifying` (retry)
/// and `NeedsClarification` (human answered, awaiting re-run).
pub(crate) fn clarifying_items(items: &[Task]) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, it)| {
            matches!(
                it.status,
                ItemStatus::Clarifying | ItemStatus::NeedsClarification
            )
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ready_item(resource: Option<&str>) -> Task {
        let mut item = Task::new("Ready task");
        item.status = ItemStatus::Queued;
        item.resource = resource.map(String::from);
        item
    }

    #[test]
    fn spawn_when_slot_available() {
        let item = make_ready_item(None);
        let result = check_dispatch(&item, 0, 10, &HashMap::new(), &HashMap::new());
        assert_eq!(result, DispatchDecision::Spawn);
    }

    #[test]
    fn no_slot_when_full() {
        let item = make_ready_item(None);
        let result = check_dispatch(&item, 10, 10, &HashMap::new(), &HashMap::new());
        assert_eq!(result, DispatchDecision::NoSlot);
    }

    #[test]
    fn resource_blocked() {
        let item = make_ready_item(Some("emulator"));
        let mut limits = HashMap::new();
        limits.insert("emulator".to_string(), 1);
        let mut counts = HashMap::new();
        counts.insert("emulator".to_string(), 1);
        let result = check_dispatch(&item, 0, 10, &limits, &counts);
        assert_eq!(result, DispatchDecision::ResourceBlocked("emulator".into()));
    }

    #[test]
    fn not_ready_status() {
        let mut item = Task::new("In progress");
        item.status = ItemStatus::InProgress;
        let result = check_dispatch(&item, 0, 10, &HashMap::new(), &HashMap::new());
        assert_eq!(result, DispatchDecision::NotReady);
    }

    #[test]
    fn rework_dispatches() {
        let mut item = Task::new("Rework task");
        item.status = ItemStatus::Rework;
        let result = check_dispatch(&item, 0, 10, &HashMap::new(), &HashMap::new());
        assert_eq!(result, DispatchDecision::Spawn);
    }

    #[test]
    fn dispatchable_rework_first() {
        let mut ready = Task::new("Ready");
        ready.status = ItemStatus::Queued;
        let mut rework = Task::new("Rework");
        rework.status = ItemStatus::Rework;

        let indices = dispatchable_items(&[ready, rework]);
        assert_eq!(indices, vec![1, 0]); // rework (idx 1) first
    }

    #[test]
    fn count_resources_basic() {
        let mut a = Task::new("A");
        a.status = ItemStatus::InProgress;
        a.resource = Some("cc".into());

        let mut b = Task::new("B");
        b.status = ItemStatus::InProgress;
        b.resource = Some("emulator".into());

        let mut c = Task::new("C");
        c.status = ItemStatus::InProgress;
        // Default resource is "cc".

        let counts = count_resources(&[a, b, c]);
        assert_eq!(counts.get("cc"), Some(&2));
        assert_eq!(counts.get("emulator"), Some(&1));
    }

    #[test]
    fn new_items_found() {
        let mut a = Task::new("A");
        a.status = ItemStatus::New;
        let mut b = Task::new("B");
        b.status = ItemStatus::Queued;
        let mut c = Task::new("C");
        c.status = ItemStatus::New;

        let result = new_items(&[a, b, c]);
        assert_eq!(result, vec![0, 2]);
    }

    #[test]
    fn clarifying_items_includes_both_statuses() {
        let mut a = Task::new("A");
        a.status = ItemStatus::Clarifying;
        let mut b = Task::new("B");
        b.status = ItemStatus::NeedsClarification;
        let mut c = Task::new("C");
        c.status = ItemStatus::Queued;
        let mut d = Task::new("D");
        d.status = ItemStatus::NeedsClarification;

        let result = clarifying_items(&[a, b, c, d]);
        assert_eq!(result, vec![0, 1, 3]);
    }
}
