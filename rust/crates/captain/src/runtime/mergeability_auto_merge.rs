//! Auto-merge gate. Consumes the captain review's confidence verdict
//! and transitions mergeable items to `CaptainMerging` when all gates pass.
//!
//! Gates (all required):
//! 1. `config.captain.auto_merge` — global settings kill-switch
//! 2. `!item.no_auto_merge` — per-task opt-out
//! 3. Item has a PR number and is not a no-PR task
//! 4. Latest `awaiting_review` event carries `confidence = "high"` and its
//!    `reviewed_head_sha` matches the PR's current head on GitHub
//!
//! The head-SHA freshness check defends against a race where the rebase
//! worker pushes a new commit after captain's review: captain's "high"
//! was about the old diff, so we must not auto-merge the new diff on its
//! authority.

use crate::{ItemStatus, Task, TimelineEvent, TimelineEventPayload};
use settings::Config;

use super::notify::Notifier;
use crate::service::lifecycle;

/// The three verdict fields the auto-merge gate consumes, read off a stored
/// `awaiting_review` timeline event.
///
/// Split out of `try_auto_merge_from_verdict` so the read is testable without
/// a GitHub round-trip: the captain-review verdict path writes these fields,
/// and this is the only place that interprets them.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ShipVerdictGateFields {
    pub confidence: String,
    pub confidence_reason: String,
    pub reviewed_head_sha: String,
}

impl ShipVerdictGateFields {
    /// `high` is the sole auto-merge grade. `mid` (and the empty sentinel
    /// written by reviews that predate the field) stays for human review.
    pub(super) fn is_high_confidence(&self) -> bool {
        self.confidence == "high"
    }
}

/// Extract the gate fields from a ship verdict event. A non-`AwaitingReview`
/// payload yields all-empty, which never passes the gate.
pub(super) fn ship_verdict_gate_fields(event: &TimelineEvent) -> ShipVerdictGateFields {
    match &event.data {
        TimelineEventPayload::AwaitingReview {
            confidence,
            confidence_reason,
            reviewed_head_sha,
            ..
        } => ShipVerdictGateFields {
            confidence: confidence.clone(),
            confidence_reason: confidence_reason.clone(),
            reviewed_head_sha: reviewed_head_sha.clone(),
        },
        _ => ShipVerdictGateFields::default(),
    }
}

/// Try to transition a mergeable item to CaptainMerging based on the
/// captain review's confidence verdict. See module doc for gates.
#[tracing::instrument(skip_all)]
pub(crate) async fn try_auto_merge_from_verdict(
    item: &mut Task,
    config: &Config,
    notifier: &Notifier,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) {
    if item.no_pr || item.no_auto_merge || item.pr_number.is_none() {
        return;
    }

    let verdict_event =
        match crate::io::queries::timeline::load_latest_ship_verdict(pool, item.id).await {
            Ok(Some(ev)) => ev,
            Ok(None) => {
                tracing::debug!(
                    module = "captain",
                    item_id = item.id,
                    "no awaiting_review event found; leaving for human review"
                );
                return;
            }
            Err(e) => {
                tracing::warn!(
                    module = "captain",
                    item_id = item.id,
                    error = %e,
                    "failed to load latest ship verdict; skipping auto-merge"
                );
                alerts.push(format!(
                    "Auto-merge verdict load failed for '{}' — {} (skipped this tick)",
                    item.title, e
                ));
                return;
            }
        };

    let gate = ship_verdict_gate_fields(&verdict_event);
    let reviewed_sha = gate.reviewed_head_sha.clone();
    if !gate.is_high_confidence() {
        tracing::debug!(
            module = "captain",
            item_id = item.id,
            confidence = %gate.confidence,
            "ship verdict not high-confidence; leaving for human review"
        );
        return;
    }

    // Freshness gate: the `awaiting_review` event stamped the head SHA
    // captain reviewed. If the PR has been pushed to since (e.g. a rebase
    // worker resolved conflicts), the reviewed diff and the mergeable diff
    // are different, so captain's "high" doesn't cover the current code.
    // Skip auto-merge and leave for human review until the next review cycle
    // writes a fresh `awaiting_review` event with the updated SHA.
    let pr_num = item.pr_number.unwrap_or(0);
    let repo = item
        .github_repo
        .clone()
        .or_else(|| settings::resolve_github_repo(Some(&item.project), config))
        .unwrap_or_default();
    match global_github::current_pr_head_sha(&repo, pr_num).await {
        Ok(current) => {
            if reviewed_sha.is_empty() || reviewed_sha == super::captain_review_payload::UNKNOWN_SHA
            {
                // Review predates the reviewed_head_sha field, was recorded
                // without a worktree, or synthesized the `unknown` sentinel.
                // Auto-merge requires a real matching SHA — fall through to
                // human review.
                tracing::debug!(
                    module = "captain",
                    item_id = item.id,
                    current = %current,
                    "ship verdict has no reviewed_head_sha; leaving for human review"
                );
                return;
            }
            if reviewed_sha != current {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    reviewed = %reviewed_sha,
                    current = %current,
                    "PR head moved after captain review; skipping auto-merge until re-review"
                );
                return;
            }
        }
        Err(e) => {
            tracing::warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to fetch current PR head SHA; skipping auto-merge this tick"
            );
            return;
        }
    }

    // All gates passed — transition to CaptainMerging so the merge spawner
    // picks it up on the next tick. Build the event first; only mutate item
    // fields after persist succeeds so a failed / idempotent-skip persist
    // leaves the in-memory task untouched for the next tick to re-evaluate.
    let confidence_reason = gate.confidence_reason;
    let pr_url = format!("https://github.com/{repo}/pull/{pr_num}");
    let prev_status = item.status;
    let prev_session_merge = item.session_ids.merge.clone();
    let prev_merge_fail_count = item.merge_fail_count;
    let prev_last_activity_at = item.last_activity_at.clone();
    if let Err(e) = lifecycle::apply_transition(item, ItemStatus::CaptainMerging) {
        tracing::warn!(
            module = "captain",
            item_id = item.id,
            error = %e,
            "illegal auto-merge transition"
        );
        return;
    }
    item.session_ids.merge = None;
    item.merge_fail_count = 0;
    item.last_activity_at = Some(global_types::now_rfc3339());
    let event = crate::TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: "High-confidence review verdict -- starting merge".to_string(),
        data: TimelineEventPayload::CaptainMergeQueued {
            pr: pr_url,
            source: "captain_review_confidence".to_string(),
            confidence_reason,
        },
    };
    match crate::io::queries::tasks::persist_status_transition(
        pool,
        item,
        prev_status.as_str(),
        &event,
    )
    .await
    {
        Ok(true) => {
            let title = global_infra::html::escape_html(&item.title);
            notifier
                .normal(&format!(
                    "\u{2705} Auto-merging <b>{title}</b> (captain review: high confidence)"
                ))
                .await;
            tracing::info!(
                module = "captain",
                item_id = item.id,
                "auto-merge transition applied from high-confidence captain review"
            );
        }
        Ok(false) => {
            lifecycle::restore_status(item, prev_status);
            item.session_ids.merge = prev_session_merge;
            item.merge_fail_count = prev_merge_fail_count;
            item.last_activity_at = prev_last_activity_at;
            tracing::debug!(
                module = "captain",
                item_id = item.id,
                "auto-merge transition already applied"
            );
        }
        Err(e) => {
            lifecycle::restore_status(item, prev_status);
            item.session_ids.merge = prev_session_merge;
            item.merge_fail_count = prev_merge_fail_count;
            item.last_activity_at = prev_last_activity_at;
            tracing::warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to persist auto-merge transition"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::captain_review::{apply_verdict, CaptainVerdict};

    /// End-to-end plumbing proof for the confidence chain:
    /// verdict(ship, high) -> `apply_verdict` -> persisted `awaiting_review`
    /// timeline payload -> the field read the auto-merge gate performs.
    ///
    /// Context: an audit reported "all 72 historical ship verdicts recorded an
    /// empty confidence". Those 72 rows are `captain_review_verdict` events
    /// (nudge / respawn / retry_clarifier), which carry no confidence by
    /// design. Ship verdicts land as `awaiting_review`, and this test pins the
    /// field actually surviving the write.
    #[tokio::test]
    async fn ship_high_confidence_reaches_the_auto_merge_gate() {
        let db = global_db::Db::open_in_memory().await.unwrap();
        let pool = db.pool().clone();
        let notifier =
            crate::runtime::notify::Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
        let workflow = settings::CaptainWorkflow::compiled_default();

        let project_id = settings::projects::upsert(&pool, "test", "", None)
            .await
            .unwrap();
        let wb_id = crate::io::test_support::seed_workbench(&pool, project_id).await;

        let mut item = crate::Task::new("confidence plumbing");
        item.project_id = project_id;
        item.project = "test".into();
        item.workbench_id = wb_id;
        item.pr_number = Some(7);
        let store = crate::io::task_store::TaskStore::new(pool.clone());
        let id = store.add(item.clone()).await.unwrap();
        item.id = id;
        store
            .update(id, |t| t.status = ItemStatus::CaptainReviewing)
            .await
            .unwrap();
        item.status = ItemStatus::CaptainReviewing;

        let verdict = CaptainVerdict {
            action: "ship".into(),
            feedback: "done".into(),
            confidence: Some("high".into()),
            confidence_reason: Some("deck 7-0.png plus the diff hunk in foo.rs".into()),
            ..Default::default()
        };
        apply_verdict(&mut item, &verdict, &workflow, &notifier, &pool)
            .await
            .unwrap();
        assert_eq!(item.status, ItemStatus::AwaitingReview);

        // `persist_status_transition` enqueues the timeline row as a lifecycle
        // outbox effect; the captain tick drains it. Drain it here so the read
        // below sees what production would see on the next tick.
        let task_store = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::io::task_store::TaskStore::new(pool.clone()),
        ));
        crate::runtime::lifecycle_effects::drain_pending(&pool, None, &task_store)
            .await
            .unwrap();

        let event = crate::io::queries::timeline::load_latest_ship_verdict(&pool, id)
            .await
            .unwrap()
            .expect("ship verdict persisted an awaiting_review event");

        // The payload itself carries the verdict's confidence — not an empty
        // sentinel — and the gate reads it as high.
        let gate = ship_verdict_gate_fields(&event);
        assert_eq!(gate.confidence, "high");
        assert_eq!(
            gate.confidence_reason,
            "deck 7-0.png plus the diff hunk in foo.rs"
        );
        assert!(gate.is_high_confidence());
    }

    /// Same chain on a `mid` verdict: it persists honestly and the gate
    /// refuses to auto-merge.
    #[tokio::test]
    async fn ship_mid_confidence_does_not_pass_the_gate() {
        let event = crate::TimelineEvent {
            timestamp: global_types::now_rfc3339(),
            actor: "captain".into(),
            summary: "Captain approved (confidence: mid)".into(),
            data: TimelineEventPayload::AwaitingReview {
                action: "ship".into(),
                feedback: "done".into(),
                confidence: "mid".into(),
                confidence_reason: "one facet rests on the worker's claim".into(),
                reviewed_head_sha: "a".repeat(40),
            },
        };
        let gate = ship_verdict_gate_fields(&event);
        assert_eq!(gate.confidence, "mid");
        assert!(!gate.is_high_confidence());
    }

    /// A non-ship payload yields all-empty fields, which can never auto-merge.
    #[test]
    fn non_ship_payload_yields_empty_gate_fields() {
        let event = crate::TimelineEvent {
            timestamp: global_types::now_rfc3339(),
            actor: "captain".into(),
            summary: "Captain nudge".into(),
            data: TimelineEventPayload::CaptainReviewVerdict {
                action: "nudge".into(),
                feedback: "add the after recording".into(),
                confidence: String::new(),
                confidence_reason: String::new(),
                reviewed_head_sha: super::super::captain_review_payload::UNKNOWN_SHA.into(),
            },
        };
        let gate = ship_verdict_gate_fields(&event);
        assert_eq!(gate, ShipVerdictGateFields::default());
        assert!(!gate.is_high_confidence());
    }

    /// `reviewed_head_sha` legitimately falls back to the `unknown` sentinel
    /// when the task has no worktree; the freshness gate then refuses to
    /// auto-merge, which is the correct conservative outcome.
    #[test]
    fn unknown_sha_sentinel_blocks_auto_merge_even_at_high_confidence() {
        let event = crate::TimelineEvent {
            timestamp: global_types::now_rfc3339(),
            actor: "captain".into(),
            summary: "Captain approved (confidence: high)".into(),
            data: TimelineEventPayload::AwaitingReview {
                action: "ship".into(),
                feedback: "done".into(),
                confidence: "high".into(),
                confidence_reason: "verified".into(),
                reviewed_head_sha: super::super::captain_review_payload::UNKNOWN_SHA.into(),
            },
        };
        let gate = ship_verdict_gate_fields(&event);
        assert!(gate.is_high_confidence());
        assert_eq!(
            gate.reviewed_head_sha,
            super::super::captain_review_payload::UNKNOWN_SHA,
            "the sentinel must survive to the freshness check that rejects it"
        );
    }
}
