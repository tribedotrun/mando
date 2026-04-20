//! Review-thread and CI-failure checking for pending-review items.

use crate::{ItemStatus, Task, TimelineEventPayload};
use rustc_hash::FxHashMap;
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

use crate::runtime::notify::Notifier;

/// Check pending-review items for unaddressed review comments and CI failures.
///
/// For each pending-review item with a PR and a worker (i.e. can be reopened):
/// - Fetch PR data (comments, CI status)
/// - If unaddressed comments → set reopen_source="review", execute ReviewReopen
/// - If CI failure on a required check → set reopen_source="ci", execute ReviewReopen
/// - If both → combined message, reopen_source="review" (comments take priority)
#[tracing::instrument(skip_all)]
pub(crate) async fn check_done_review_threads(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) {
    // Collect indices of pending-review items with a PR and worktree (needed for reopen).
    let candidates: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| {
            it.status == ItemStatus::AwaitingReview
                && it.pr_number.is_some()
                && it.worktree.is_some()
        })
        .map(|(i, _)| i)
        .collect();

    for idx in candidates {
        // Re-check status — a prior iteration may have mutated this item.
        if items[idx].status != ItemStatus::AwaitingReview {
            continue;
        }

        // Build a stub with the resolved github_repo slug so fetch_pr_data
        // can resolve short PR refs like "#334" to the correct repo.
        let github_repo = settings::config::resolve_github_repo(Some(&items[idx].project), config);
        let stub = Task {
            pr_number: items[idx].pr_number,
            github_repo,
            ..Task::new("")
        };
        let pr_data = super::review_phase::fetch_pr_data(&stub).await;

        // CI failure → CaptainReviewing with ci_failure trigger.
        // Captain reads CI logs and decides how to proceed (precise reopen or escalate).
        let has_ci_failure = pr_data.ci_status.as_deref() == Some("failure");
        if has_ci_failure {
            let item = &mut items[idx];
            let snap = super::action_contract::ReviewFieldsSnapshot::capture(item);
            let title = global_infra::html::escape_html(&item.title);
            tracing::info!(
                module = "captain",
                title = %item.title,
                "CI failure detected on pending-review item — spawning captain review"
            );
            super::action_contract::reset_review_retry(item, crate::ReviewTrigger::CiFailure);

            let event = crate::TimelineEvent {
                timestamp: global_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: "CI failure detected — captain reviewing".to_string(),
                data: TimelineEventPayload::CaptainReviewCiFailure {
                    trigger: "ci_failure".to_string(),
                },
            };
            match crate::io::queries::tasks::persist_status_transition(
                pool,
                item,
                snap.status.as_str(),
                &event,
            )
            .await
            {
                Ok(true) => {
                    notifier
                        .normal(&format!(
                            "\u{1f6a8} CI failing on <b>{title}</b> — captain investigating"
                        ))
                        .await;
                }
                Ok(false) => {
                    tracing::info!(module = "captain", "CI failure transition already applied");
                }
                Err(e) => {
                    snap.restore(item);
                    tracing::error!(module = "captain", error = %e, "persist failed for CI failure");
                }
            }
            continue;
        }

        // Query artifact gates from DB.
        let artifacts = crate::io::queries::artifacts::list_for_task(pool, items[idx].id)
            .await
            .unwrap_or_default();
        let has_evidence_db = artifacts
            .iter()
            .any(|a| a.artifact_type == crate::ArtifactType::Evidence);
        let evidence_fresh_db = if items[idx].reopen_seq == 0 || items[idx].reopened_at.is_none() {
            has_evidence_db
        } else {
            let threshold = items[idx].reopened_at.as_deref().unwrap_or("");
            artifacts.iter().any(|a| {
                a.artifact_type == crate::ArtifactType::Evidence
                    && a.created_at.as_str() > threshold
            })
        };

        let decision = classify_review_state(
            pr_data.ci_status.as_deref(),
            pr_data.unresolved_threads,
            pr_data.unreplied_threads,
            pr_data.unaddressed_issue_comments,
            &pr_data.body,
            workflow,
            items[idx].reopen_seq,
            &pr_data.head_sha,
            has_evidence_db,
            evidence_fresh_db,
        );

        let (reopen_source, message) = match decision {
            Some(d) => d,
            None => continue,
        };

        // Worker and session_ids.worker should be preserved from pending-review.
        // If either is missing, skip the reopen and emit an alert; generating
        // fake identifiers would break reopen (resume-nonexistent-session) and
        // leak PIDs.
        if items[idx].worker.is_none() || items[idx].session_ids.worker.is_none() {
            let item_id = items[idx].id.to_string();
            let missing = match (
                items[idx].worker.is_some(),
                items[idx].session_ids.worker.is_some(),
            ) {
                (false, false) => "worker and session_id",
                (false, true) => "worker",
                (true, false) => "session_id",
                (true, true) => unreachable!(),
            };
            let msg =
                format!("Skipping reopen on pending-review item {item_id}: missing {missing}");
            tracing::error!(module = "captain", %item_id, %missing, "{msg}");
            alerts.push(msg);
            continue;
        }

        let item = &mut items[idx];
        let worker_name = item.worker.clone().unwrap_or_default();

        tracing::info!(
            module = "captain",
            title = %item.title,
            worker = %worker_name,
            %reopen_source,
            unresolved = pr_data.unresolved_threads,
            unreplied = pr_data.unreplied_threads,
            unaddressed = pr_data.unaddressed_issue_comments,
            ci_status = pr_data.ci_status.as_deref().unwrap_or("none"),
            "review-reopening pending-review item"
        );

        match super::action_contract::reopen_item(
            item,
            &reopen_source,
            &message,
            config,
            workflow,
            notifier,
            pool,
            false,
        )
        .await
        {
            Ok(super::action_contract::ReopenOutcome::Reopened) => {
                let it = &mut items[idx];
                let seq = it.reopen_seq;

                let msg = format!(
                    "\u{1f504} {}-reopened (seq={}): <b>{}</b>\n{}",
                    reopen_source,
                    seq,
                    global_infra::html::escape_html(&it.title),
                    global_infra::html::escape_html(&message),
                );
                notifier.high(&msg).await;
            }
            Ok(super::action_contract::ReopenOutcome::CaptainReviewing) => {}
            Ok(super::action_contract::ReopenOutcome::QueuedFallback) => {
                alerts.push(format!(
                    "Review-reopen for {} fell back to queued unexpectedly",
                    worker_name
                ));
            }
            Err(e) => {
                alerts.push(format!("Review-reopen failed for {}: {}", worker_name, e));
            }
        }
    }
}

/// Decide whether a pending-review item needs reopening based on PR state.
///
/// CI failures are handled upstream (CaptainReviewing with ci_failure trigger)
/// before this function is called. This only handles review comments and
/// missing evidence.
#[allow(clippy::too_many_arguments)]
pub(crate) fn classify_review_state(
    _ci_status: Option<&str>,
    unresolved: i64,
    unreplied: i64,
    unaddressed: i64,
    _pr_body: &str,
    workflow: &CaptainWorkflow,
    reopen_seq: i64,
    _pr_head_sha: &str,
    has_evidence_db: bool,
    evidence_fresh_db: bool,
) -> Option<(String, String)> {
    let has_comments = unresolved > 0 || unreplied > 0 || unaddressed > 0;

    // DB-backed evidence checks replace PR body parsing.
    let missing_evidence = !has_evidence_db;
    let stale_evidence = has_evidence_db && reopen_seq > 0 && !evidence_fresh_db;

    if !has_comments && !missing_evidence && !stale_evidence {
        return None;
    }

    let mut parts = Vec::new();

    let reopen_source = if has_comments {
        let mut detail = Vec::new();
        if unresolved > 0 {
            detail.push(format!("{unresolved} unresolved threads"));
        }
        if unreplied > 0 {
            detail.push(format!("{unreplied} unreplied threads"));
        }
        if unaddressed > 0 {
            detail.push(format!("{unaddressed} unaddressed issue comments"));
        }
        parts.push(format!(
            "Unaddressed review feedback: {}",
            detail.join(", ")
        ));
        "review".to_string()
    } else {
        // missing_evidence or stale_evidence without comments
        "evidence".to_string()
    };

    if missing_evidence {
        parts.push(
            "Task is missing runtime evidence. Capture evidence (screenshot for UI, \
             terminal output for non-UI) and save with `mando todo evidence <file> \
             --caption \"...\"`."
                .to_string(),
        );
    }

    if stale_evidence {
        parts.push(
            "Evidence is stale -- it was captured before the last reopen and no longer \
             reflects the current code. Recapture and save with `mando todo evidence`."
                .to_string(),
        );
    }

    let issues_text = parts.join("\n");
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("issues", issues_text.as_str());
    let message = match settings::config::render_prompt(
        "review_reopen_message",
        &workflow.prompts,
        &vars,
    ) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(module = "captain", error = %e, "failed to render review_reopen_message");
            return None;
        }
    };
    Some((reopen_source, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wf() -> CaptainWorkflow {
        CaptainWorkflow::compiled_default()
    }

    // Helper: call classify_review_state with DB-backed evidence flags.
    fn classify(
        ci: Option<&str>,
        unresolved: i64,
        unreplied: i64,
        unaddressed: i64,
        reopen_seq: i64,
        has_evidence: bool,
        evidence_fresh: bool,
    ) -> Option<(String, String)> {
        classify_review_state(
            ci,
            unresolved,
            unreplied,
            unaddressed,
            "",
            &wf(),
            reopen_seq,
            "",
            has_evidence,
            evidence_fresh,
        )
    }

    #[test]
    fn clean_pr_returns_none() {
        assert!(classify(Some("success"), 0, 0, 0, 0, true, true).is_none());
    }

    #[test]
    fn pending_ci_returns_none() {
        assert!(classify(Some("pending"), 0, 0, 0, 0, true, true).is_none());
    }

    #[test]
    fn no_ci_no_comments_returns_none() {
        assert!(classify(None, 0, 0, 0, 0, true, true).is_none());
    }

    #[test]
    fn unresolved_threads_trigger_review_reopen() {
        let (source, msg) = classify(Some("success"), 3, 0, 0, 0, true, true).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("3 unresolved threads"));
    }

    #[test]
    fn unreplied_threads_trigger_review_reopen() {
        let (source, msg) = classify(Some("success"), 0, 2, 0, 0, true, true).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("2 unreplied threads"));
    }

    #[test]
    fn unaddressed_issue_comments_trigger_review_reopen() {
        let (source, msg) = classify(Some("success"), 0, 0, 1, 0, true, true).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("1 unaddressed issue comments"));
    }

    #[test]
    fn ci_failure_alone_returns_none_handled_upstream() {
        assert!(classify(Some("failure"), 0, 0, 0, 0, true, true).is_none());
    }

    #[test]
    fn comments_with_ci_failure_still_triggers_review_reopen() {
        let (source, msg) = classify(Some("failure"), 1, 0, 2, 0, true, true).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("1 unresolved threads"));
        assert!(msg.contains("2 unaddressed issue comments"));
    }

    #[test]
    fn no_ci_data_no_comments_returns_none() {
        assert!(classify(None, 0, 0, 0, 0, true, true).is_none());
    }

    #[test]
    fn missing_evidence_triggers_reopen() {
        let (source, msg) = classify(Some("success"), 0, 0, 0, 0, false, false).unwrap();
        assert_eq!(source, "evidence");
        assert!(msg.contains("missing runtime evidence"));
    }

    #[test]
    fn missing_evidence_with_threads_uses_review_source() {
        let (source, msg) = classify(Some("success"), 1, 0, 0, 0, false, false).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("missing runtime evidence"));
        assert!(msg.contains("1 unresolved threads"));
    }

    #[test]
    fn stale_evidence_triggers_reopen() {
        let (source, msg) = classify(Some("success"), 0, 0, 0, 1, true, false).unwrap();
        assert_eq!(source, "evidence");
        assert!(msg.contains("stale"));
    }

    #[test]
    fn fresh_evidence_after_reopen_returns_none() {
        assert!(classify(Some("success"), 0, 0, 0, 1, true, true).is_none());
    }

    #[test]
    fn stale_evidence_with_threads_uses_review_source() {
        let (source, msg) = classify(Some("success"), 1, 0, 0, 1, true, false).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("stale"));
        assert!(msg.contains("1 unresolved threads"));
    }
}
