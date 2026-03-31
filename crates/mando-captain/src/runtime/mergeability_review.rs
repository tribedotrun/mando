//! Review-thread and CI-failure checking for pending-review items.

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};

use crate::runtime::linear_integration;
use crate::runtime::notify::Notifier;

/// Check pending-review items for unaddressed review comments and CI failures.
///
/// For each pending-review item with a PR and a worker (i.e. can be reopened):
/// - Fetch PR data (comments, CI status)
/// - If unaddressed comments → set reopen_source="review", execute ReviewReopen
/// - If CI failure on a required check → set reopen_source="ci", execute ReviewReopen
/// - If both → combined message, reopen_source="review" (comments take priority)
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
            it.status == ItemStatus::AwaitingReview && it.pr.is_some() && it.worktree.is_some()
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
        let github_repo = mando_config::resolve_github_repo(items[idx].project.as_deref(), config);
        let stub = Task {
            pr: items[idx].pr.clone(),
            project: github_repo,
            ..Task::new("")
        };
        let pr_data = super::review_phase::fetch_pr_data(&stub).await;

        // CI failure → CaptainReviewing with ci_failure trigger.
        // Captain reads CI logs and decides how to proceed (precise reopen or escalate).
        let has_ci_failure = pr_data.ci_status.as_deref() == Some("failure");
        if has_ci_failure {
            let item = &mut items[idx];
            let title = mando_shared::telegram_format::escape_html(&item.title);
            tracing::info!(
                module = "captain",
                title = %item.title,
                "CI failure detected on pending-review item — spawning captain review"
            );
            super::action_contract::reset_review_retry(
                item,
                mando_types::task::ReviewTrigger::CiFailure,
            );

            super::timeline_emit::emit_for_task(
                item,
                mando_types::timeline::TimelineEventType::CaptainReviewStarted,
                "CI failure detected — captain reviewing",
                serde_json::json!({ "trigger": "ci_failure" }),
                pool,
            )
            .await;
            notifier
                .normal(&format!(
                    "\u{1f6a8} CI failing on <b>{title}</b> — captain investigating"
                ))
                .await;
            continue;
        }

        let decision = classify_review_state(
            pr_data.ci_status.as_deref(),
            pr_data.unresolved_threads,
            pr_data.unreplied_threads,
            pr_data.unaddressed_issue_comments,
            &pr_data.body,
            workflow,
        );

        let (reopen_source, message) = match decision {
            Some(d) => d,
            None => continue,
        };

        // Worker and session_ids.worker should be preserved from pending-review.
        // Fall back to generated values only as a safety net.
        if items[idx].worker.is_none() {
            let item_id = items[idx].best_id();
            tracing::warn!(module = "captain", %item_id, "worker missing on pending-review item — generating fallback name");
            items[idx].worker = Some(format!("mando-reopen-{}", item_id));
        }
        if items[idx].session_ids.worker.is_none() {
            let item_id = items[idx].best_id();
            tracing::warn!(
                module = "captain",
                %item_id,
                "worker session_id missing on pending-review item — generating fresh session"
            );
            items[idx].session_ids.worker = Some(mando_uuid::Uuid::v4().to_string());
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
                    mando_shared::telegram_format::escape_html(&it.title),
                    mando_shared::telegram_format::escape_html(&message),
                );
                notifier.high(&msg).await;

                if let Err(e) = linear_integration::writeback_status(it, config).await {
                    tracing::warn!(module = "captain", %e, "Linear status writeback failed");
                }
                if let Err(e) = linear_integration::upsert_workpad(
                    it,
                    config,
                    &format!(
                        "{} reopened (seq={}), working on feedback",
                        reopen_source, seq
                    ),
                    pool,
                )
                .await
                {
                    tracing::warn!(module = "captain", %e, "Linear workpad upsert failed");
                }
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
pub(crate) fn classify_review_state(
    _ci_status: Option<&str>,
    unresolved: i64,
    unreplied: i64,
    unaddressed: i64,
    pr_body: &str,
    workflow: &CaptainWorkflow,
) -> Option<(String, String)> {
    let has_comments = unresolved > 0 || unreplied > 0 || unaddressed > 0;

    // Check for missing evidence using the same logic as deterministic classifier.
    let missing_evidence = crate::biz::worker_context::has_no_evidence(pr_body);

    if !has_comments && !missing_evidence {
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
        "evidence".to_string()
    };

    if missing_evidence {
        parts.push(
            "PR is missing runtime evidence. After evidence is required: screenshot for \
             static UI changes, recording for interactive/multi-step changes, terminal \
             output under `### After` for non-UI changes. NOT diagrams, ASCII art, or \
             test results alone."
                .to_string(),
        );
    }

    let issues_text = parts.join("\n");
    let mut vars = std::collections::HashMap::new();
    vars.insert("issues", issues_text.as_str());
    let message = match mando_config::render_prompt(
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

    /// PR body with valid evidence (heading + image URL).
    const BODY_WITH_EVIDENCE: &str = "### After\n![fix](https://example.com/fix.png)";
    /// PR body without evidence.
    const BODY_NO_EVIDENCE: &str = "## Summary\nJust a description";

    fn wf() -> CaptainWorkflow {
        CaptainWorkflow::compiled_default()
    }

    #[test]
    fn clean_pr_returns_none() {
        let result = classify_review_state(Some("success"), 0, 0, 0, BODY_WITH_EVIDENCE, &wf());
        assert!(result.is_none());
    }

    #[test]
    fn pending_ci_returns_none() {
        let result = classify_review_state(Some("pending"), 0, 0, 0, BODY_WITH_EVIDENCE, &wf());
        assert!(result.is_none());
    }

    #[test]
    fn no_ci_no_comments_returns_none() {
        let result = classify_review_state(None, 0, 0, 0, BODY_WITH_EVIDENCE, &wf());
        assert!(result.is_none());
    }

    #[test]
    fn unresolved_threads_trigger_review_reopen() {
        let (source, msg) =
            classify_review_state(Some("success"), 3, 0, 0, BODY_WITH_EVIDENCE, &wf()).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("3 unresolved threads"));
        assert!(!msg.contains("CI"));
    }

    #[test]
    fn unreplied_threads_trigger_review_reopen() {
        let (source, msg) =
            classify_review_state(Some("success"), 0, 2, 0, BODY_WITH_EVIDENCE, &wf()).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("2 unreplied threads"));
    }

    #[test]
    fn unaddressed_issue_comments_trigger_review_reopen() {
        let (source, msg) =
            classify_review_state(Some("success"), 0, 0, 1, BODY_WITH_EVIDENCE, &wf()).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("1 unaddressed issue comments"));
    }

    #[test]
    fn ci_failure_alone_returns_none_handled_upstream() {
        // CI failures are now handled upstream via CaptainReviewing ci_failure trigger.
        // classify_review_state should return None for CI-only failures.
        let result = classify_review_state(Some("failure"), 0, 0, 0, BODY_WITH_EVIDENCE, &wf());
        assert!(result.is_none());
    }

    #[test]
    fn comments_with_ci_failure_still_triggers_review_reopen() {
        // Comments take priority — CI failure is handled upstream, but comments
        // still trigger a review reopen through classify_review_state.
        let (source, msg) =
            classify_review_state(Some("failure"), 1, 0, 2, BODY_WITH_EVIDENCE, &wf()).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("1 unresolved threads"));
        assert!(msg.contains("2 unaddressed issue comments"));
    }

    #[test]
    fn no_ci_data_no_comments_returns_none() {
        let result = classify_review_state(None, 0, 0, 0, BODY_WITH_EVIDENCE, &wf());
        assert!(result.is_none());
    }

    #[test]
    fn missing_evidence_triggers_reopen() {
        let (source, msg) =
            classify_review_state(Some("success"), 0, 0, 0, BODY_NO_EVIDENCE, &wf()).unwrap();
        assert_eq!(source, "evidence");
        assert!(msg.contains("missing runtime evidence"));
    }

    #[test]
    fn missing_evidence_with_threads_uses_review_source() {
        let (source, msg) =
            classify_review_state(Some("success"), 1, 0, 0, BODY_NO_EVIDENCE, &wf()).unwrap();
        assert_eq!(source, "review");
        assert!(msg.contains("missing runtime evidence"));
        assert!(msg.contains("1 unresolved threads"));
    }
}
