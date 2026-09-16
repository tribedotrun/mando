//! Dispatch logic — ready→in-progress slot allocation.

use std::collections::HashMap;

use crate::{ItemStatus, Task};

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

    // Check resource-specific limits. `None` means the task consumes no scarce
    // shared resource and is governed only by global/provider scheduling caps.
    if let Some(resource) = item.resource.as_deref() {
        if let Some(&limit) = resource_limits.get(resource) {
            let current = resource_counts.get(resource).copied().unwrap_or(0);
            if current >= limit {
                return DispatchDecision::ResourceBlocked(resource.to_string());
            }
        }
    }

    DispatchDecision::Spawn
}

/// Count active resources across in-progress items.
pub(crate) fn count_resources(items: &[Task]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for item in items {
        if item.status == ItemStatus::InProgress {
            if let Some(resource) = item.resource.as_deref() {
                *counts.entry(resource.to_string()).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// Count items currently occupying each per-state cap, keyed by kebab-case
/// wire name. The `InProgress` predicate matches the existing global
/// counter at `tick.rs::run_captain_tick_inner` (`worker.is_some()`);
/// the other three states mirror the same shape on their
/// own session id field. A candidate that already transitioned but
/// hasn't spawned yet does not self-block because it has no session id.
pub(crate) fn count_active_states(items: &[Task]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for item in items {
        let wire = match item.status {
            ItemStatus::InProgress if item.worker.is_some() => IN_PROGRESS_WIRE,
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
/// Items with status `ready` or `rework` are eligible.
/// Sorted by: rework first, then by creation order (position in list).
pub(crate) fn dispatchable_items(items: &[Task]) -> Vec<usize> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let mut candidates: Vec<(usize, bool)> = Vec::new();

    for (i, item) in items.iter().enumerate() {
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
