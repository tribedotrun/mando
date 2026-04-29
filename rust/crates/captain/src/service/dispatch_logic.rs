//! Dispatch logic — ready→in-progress slot allocation.

use std::collections::HashMap;

use crate::{ItemStatus, Task};

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
    /// Item blocked by per-state concurrency cap (kebab-case wire name).
    StateBlocked(String),
    /// Item not dispatchable (wrong status, etc.).
    NotReady,
}

/// Wire name for the `in-progress` state — the target state for any item
/// dispatched by `check_dispatch`. Mirrors `ItemStatus::InProgress.as_str()`
/// but lives here so the per-state cap lookup is local to the dispatcher.
pub(crate) const IN_PROGRESS_WIRE: &str = "in-progress";

/// Check if a ready item can be dispatched.
///
/// Returns `Spawn` when a slot is available, otherwise the specific reason
/// it was blocked (`NoSlot`, `ResourceBlocked`, or `StateBlocked`).
pub(crate) fn check_dispatch(
    item: &Task,
    active_workers: usize,
    max_workers: usize,
    resource_limits: &HashMap<String, usize>,
    resource_counts: &HashMap<String, usize>,
    per_state_limits: &HashMap<String, usize>,
    state_counts: &HashMap<String, usize>,
) -> DispatchDecision {
    match item.status {
        ItemStatus::Queued | ItemStatus::Rework => {}
        _ => return DispatchDecision::NotReady,
    };

    if active_workers >= max_workers {
        return DispatchDecision::NoSlot;
    }

    // Per-state cap for `in-progress` (the state this item would enter on
    // spawn). The global `max_concurrent` already bounds total in-progress
    // workers; per-state lets operators set a tighter ceiling without
    // touching the global cap.
    if let Some(&limit) = per_state_limits.get(IN_PROGRESS_WIRE) {
        let current = state_counts.get(IN_PROGRESS_WIRE).copied().unwrap_or(0);
        if current >= limit {
            return DispatchDecision::StateBlocked(IN_PROGRESS_WIRE.to_string());
        }
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

/// Count items currently occupying each per-state cap, keyed by kebab-case
/// wire name. The `InProgress` predicate matches the existing global
/// counter at `tick.rs::run_captain_tick_inner` (`worker.is_some() &&
/// !planning`); the other three states mirror the same shape on their
/// own session id field. A candidate that already transitioned but
/// hasn't spawned yet does not self-block because it has no session id.
pub(crate) fn count_active_states(items: &[Task]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for item in items {
        let wire = match item.status {
            ItemStatus::InProgress if item.worker.is_some() && !item.planning => IN_PROGRESS_WIRE,
            ItemStatus::Clarifying if item.session_ids.clarifier.is_some() => "clarifying",
            ItemStatus::CaptainReviewing if item.session_ids.review.is_some() => {
                "captain-reviewing"
            }
            ItemStatus::CaptainMerging if item.session_ids.merge.is_some() => "captain-merging",
            _ => continue,
        };
        *counts.entry(wire.to_string()).or_insert(0) += 1;
    }
    counts
}

/// Find items eligible for regular worker dispatch, in priority order.
///
/// Items with status `ready` or `rework` are eligible. Planning-mode items
/// are excluded (dispatched separately by `dispatch_planning`).
/// Sorted by: rework first, then by creation order (position in list).
pub(crate) fn dispatchable_items(items: &[Task]) -> Vec<usize> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let mut candidates: Vec<(usize, bool)> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        if item.planning {
            continue;
        }
        // Skip paused tasks (credential pool exhausted on a prior tick) —
        // they rejoin the candidate set once `paused_until` has passed.
        if item.paused_until.is_some_and(|until| until > now) {
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
///
/// Skips tasks whose `paused_until` is still in the future — those tasks
/// parked themselves after `AllCredentialsExhausted` on a prior tick and
/// must wait for the soonest credential cooldown to pass before captain
/// re-dispatches. Past or unset `paused_until` is treated as eligible.
pub(crate) fn new_items(items: &[Task]) -> Vec<usize> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.status == ItemStatus::New)
        .filter(|(_, it)| it.paused_until.is_none_or(|until| until <= now))
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

    fn check(
        item: &Task,
        active: usize,
        max: usize,
        res_limits: &HashMap<String, usize>,
        res_counts: &HashMap<String, usize>,
    ) -> DispatchDecision {
        check_dispatch(
            item,
            active,
            max,
            res_limits,
            res_counts,
            &HashMap::new(),
            &HashMap::new(),
        )
    }

    #[test]
    fn spawn_when_slot_available() {
        let item = make_ready_item(None);
        let result = check(&item, 0, 10, &HashMap::new(), &HashMap::new());
        assert_eq!(result, DispatchDecision::Spawn);
    }

    #[test]
    fn no_slot_when_full() {
        let item = make_ready_item(None);
        let result = check(&item, 10, 10, &HashMap::new(), &HashMap::new());
        assert_eq!(result, DispatchDecision::NoSlot);
    }

    #[test]
    fn resource_blocked() {
        let item = make_ready_item(Some("emulator"));
        let mut limits = HashMap::new();
        limits.insert("emulator".to_string(), 1);
        let mut counts = HashMap::new();
        counts.insert("emulator".to_string(), 1);
        let result = check(&item, 0, 10, &limits, &counts);
        assert_eq!(result, DispatchDecision::ResourceBlocked("emulator".into()));
    }

    #[test]
    fn not_ready_status() {
        let mut item = Task::new("In progress");
        item.status = ItemStatus::InProgress;
        let result = check(&item, 0, 10, &HashMap::new(), &HashMap::new());
        assert_eq!(result, DispatchDecision::NotReady);
    }

    #[test]
    fn rework_dispatches() {
        let mut item = Task::new("Rework task");
        item.status = ItemStatus::Rework;
        let result = check(&item, 0, 10, &HashMap::new(), &HashMap::new());
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

    fn live(status: ItemStatus) -> Task {
        let mut t = Task::new("live");
        t.status = status;
        match status {
            ItemStatus::InProgress => {
                t.worker = Some("w-1".into());
                t.session_ids.worker = Some("sess-w".into());
            }
            ItemStatus::Clarifying => t.session_ids.clarifier = Some("sess-c".into()),
            ItemStatus::CaptainReviewing => t.session_ids.review = Some("sess-r".into()),
            ItemStatus::CaptainMerging => t.session_ids.merge = Some("sess-m".into()),
            _ => {}
        }
        t
    }

    #[test]
    fn count_active_states_buckets_by_wire_name() {
        let items = [
            live(ItemStatus::InProgress),
            live(ItemStatus::InProgress),
            live(ItemStatus::Clarifying),
            live(ItemStatus::CaptainReviewing),
            live(ItemStatus::CaptainMerging),
        ];
        let counts = count_active_states(&items);
        assert_eq!(counts.get("in-progress"), Some(&2));
        assert_eq!(counts.get("clarifying"), Some(&1));
        assert_eq!(counts.get("captain-reviewing"), Some(&1));
        assert_eq!(counts.get("captain-merging"), Some(&1));
    }

    #[test]
    fn count_active_states_excludes_planning_and_sessionless() {
        // InProgress with no session id (e.g. transitioning) shouldn't count.
        let mut a = Task::new("a");
        a.status = ItemStatus::InProgress;
        // worker + session_ids.worker missing
        // InProgress + planning excluded.
        let mut b = Task::new("b");
        b.status = ItemStatus::InProgress;
        b.worker = Some("w".into());
        b.session_ids.worker = Some("sess".into());
        b.planning = true;
        // Captain reviewing without a session id (transitional).
        let mut c = Task::new("c");
        c.status = ItemStatus::CaptainReviewing;

        let counts = count_active_states(&[a, b, c]);
        assert!(counts.is_empty(), "got {:?}", counts);
    }

    #[test]
    fn state_blocked_when_in_progress_cap_full() {
        let item = make_ready_item(None);
        let mut state_limits = HashMap::new();
        state_limits.insert("in-progress".to_string(), 2);
        let mut state_counts = HashMap::new();
        state_counts.insert("in-progress".to_string(), 2);
        let decision = check_dispatch(
            &item,
            0,
            10,
            &HashMap::new(),
            &HashMap::new(),
            &state_limits,
            &state_counts,
        );
        assert_eq!(
            decision,
            DispatchDecision::StateBlocked("in-progress".into())
        );
    }

    #[test]
    fn state_cap_under_limit_allows_spawn() {
        let item = make_ready_item(None);
        let mut state_limits = HashMap::new();
        state_limits.insert("in-progress".to_string(), 5);
        let mut state_counts = HashMap::new();
        state_counts.insert("in-progress".to_string(), 2);
        let decision = check_dispatch(
            &item,
            2,
            10,
            &HashMap::new(),
            &HashMap::new(),
            &state_limits,
            &state_counts,
        );
        assert_eq!(decision, DispatchDecision::Spawn);
    }

    #[test]
    fn global_cap_dominates_when_more_restrictive() {
        // Global cap is 1, per-state cap is 5. Global wins.
        let item = make_ready_item(None);
        let mut state_limits = HashMap::new();
        state_limits.insert("in-progress".to_string(), 5);
        let state_counts = HashMap::new();
        let decision = check_dispatch(
            &item,
            1,
            1,
            &HashMap::new(),
            &HashMap::new(),
            &state_limits,
            &state_counts,
        );
        assert_eq!(decision, DispatchDecision::NoSlot);
    }

    #[test]
    fn empty_per_state_limits_matches_legacy_behaviour() {
        // Regression guard: empty map = no per-state gating, same outcome
        // as before this field existed.
        let item = make_ready_item(None);
        let decision = check_dispatch(
            &item,
            0,
            10,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(decision, DispatchDecision::Spawn);
    }
}
