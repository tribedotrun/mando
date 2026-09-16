//! Review-thread and CI-failure checking for pending-review items.

use crate::{ItemStatus, Task, TimelineEventPayload};
use rustc_hash::FxHashMap;
use settings::CaptainWorkflow;
use settings::Config;

use crate::runtime::notify::Notifier;

/// Visible-text budget for the LLM-authored verdict feedback body. 1500
/// keeps headroom under TG's 4096-char message cap once the title and
/// structural prefix are added.
const REOPEN_BODY_VISIBLE_BUDGET: usize = 1500;

/// Build the review-reopen notification message: a structural HTML prefix
/// with an escaped title plus a renderer-formatted markdown verdict body.
fn format_reopen_notification(
    reopen_source: &str,
    seq: i64,
    title: &str,
    message_markdown: &str,
) -> String {
    let body = global_infra::tg_markdown::render_markdown_reply_html(
        message_markdown,
        REOPEN_BODY_VISIBLE_BUDGET,
    );
    format!(
        "\u{1f504} {reopen_source}-reopened (seq={seq}): <b>{}</b>\n{body}",
        global_infra::html::escape_html(title),
    )
}

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
        let github_repo = settings::resolve_github_repo(Some(&items[idx].project), config);
        let stub = Task {
            pr_number: items[idx].pr_number,
            github_repo,
            ..Task::new("")
        };
        let pr_data = super::review_phase::fetch_pr_data(&stub).await;

        let draft_decision = if pr_data.is_draft {
            classify_review_state(
                true,
                pr_data.unresolved_threads,
                pr_data.unreplied_threads,
                pr_data.unaddressed_issue_comments,
                workflow,
                items[idx].reopen_seq,
                true,
                true,
            )
        } else {
            None
        };

        // CI failure → CaptainReviewing with ci_failure trigger.
        // Captain reads CI logs and decides how to proceed (precise reopen or escalate).
        let has_ci_failure = pr_data.ci_status.as_deref() == Some("failure");
        if draft_decision.is_none() && has_ci_failure {
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

        let decision = if draft_decision.is_some() {
            draft_decision
        } else {
            // Query artifact gates from DB.
            let artifacts = crate::io::queries::artifacts::list_for_task(pool, items[idx].id)
                .await
                .unwrap_or_default();
            let data_dir = global_infra::paths::data_dir();
            let has_evidence_db = artifacts
                .iter()
                .any(|artifact| evidence_media_exists(artifact, &data_dir));
            let evidence_fresh_db =
                if items[idx].reopen_seq == 0 || items[idx].reopened_at.is_none() {
                    has_evidence_db
                } else {
                    let threshold = items[idx].reopened_at.as_deref().unwrap_or("");
                    artifacts.iter().any(|artifact| {
                        evidence_media_exists(artifact, &data_dir)
                            && artifact.created_at.as_str() > threshold
                    })
                };

            classify_review_state(
                false,
                pr_data.unresolved_threads,
                pr_data.unreplied_threads,
                pr_data.unaddressed_issue_comments,
                workflow,
                items[idx].reopen_seq,
                has_evidence_db,
                evidence_fresh_db,
            )
        };

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

                let msg = format_reopen_notification(&reopen_source, seq, &it.title, &message);
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

fn evidence_media_exists(artifact: &crate::TaskArtifact, data_dir: &std::path::Path) -> bool {
    artifact.artifact_type == crate::ArtifactType::Evidence
        && !artifact.media.is_empty()
        && artifact.media.iter().all(|media| {
            media.local_path.as_deref().is_some_and(|local_path| {
                let relative = std::path::Path::new(local_path);
                !relative.is_absolute()
                    && !relative
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                    && data_dir.join(relative).is_file()
            })
        })
}

/// Decide whether a pending-review item needs reopening based on PR state.
///
/// CI failures are handled upstream (CaptainReviewing with ci_failure trigger)
/// before this function is called. This handles draft PRs, review comments,
/// and missing evidence.
#[allow(clippy::too_many_arguments)]
pub(crate) fn classify_review_state(
    is_draft: bool,
    unresolved: i64,
    unreplied: i64,
    unaddressed: i64,
    workflow: &CaptainWorkflow,
    reopen_seq: i64,
    has_evidence_db: bool,
    evidence_fresh_db: bool,
) -> Option<(String, String)> {
    if is_draft {
        let issues_text = issue_text(workflow, "mergeability_issue_draft")?;
        let message = reopen_message(workflow, &issues_text)?;
        return Some(("draft".to_string(), message));
    }

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
        parts.push(issue_text(workflow, "mergeability_issue_missing_evidence")?);
    }

    if stale_evidence {
        parts.push(issue_text(workflow, "mergeability_issue_stale_evidence")?);
    }

    let issues_text = parts.join("\n");
    let message = reopen_message(workflow, &issues_text)?;
    Some((reopen_source, message))
}

/// One of the `mergeability_issue_*` bodies from the workflow YAML.
fn issue_text(workflow: &CaptainWorkflow, key: &'static str) -> Option<String> {
    let vars: FxHashMap<&str, &str> = FxHashMap::default();
    match settings::render_prompt(key, &workflow.prompts, &vars) {
        Ok(text) => Some(text.trim().to_string()),
        Err(e) => {
            tracing::error!(module = "captain", prompt = key, error = %e, "failed to render mergeability issue text");
            None
        }
    }
}

fn reopen_message(workflow: &CaptainWorkflow, issues: &str) -> Option<String> {
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("issues", issues);
    match settings::render_prompt("review_reopen_message", &workflow.prompts, &vars) {
        Ok(message) => Some(message),
        Err(e) => {
            tracing::error!(module = "captain", error = %e, "failed to render review_reopen_message");
            None
        }
    }
}
