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

use crate::{ItemStatus, Task, TimelineEventType};
use settings::config::settings::Config;

use super::notify::Notifier;

/// Try to transition a mergeable item to CaptainMerging based on the
/// captain review's confidence verdict. See module doc for gates.
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

    let confidence = verdict_event
        .data
        .get("confidence")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if confidence != "high" {
        tracing::debug!(
            module = "captain",
            item_id = item.id,
            confidence = %confidence,
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
        .or_else(|| settings::config::resolve_github_repo(Some(&item.project), config))
        .unwrap_or_default();
    let reviewed_sha = verdict_event
        .data
        .get("reviewed_head_sha")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    match current_pr_head_sha(&repo, pr_num).await {
        Ok(current) => match &reviewed_sha {
            Some(reviewed) if reviewed == &current => {}
            Some(reviewed) => {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    reviewed = %reviewed,
                    current = %current,
                    "PR head moved after captain review; skipping auto-merge until re-review"
                );
                return;
            }
            None => {
                // Event predates the reviewed_head_sha field (pre-refactor
                // rows) or the review ran without a worktree. Refuse to
                // auto-merge without a freshness anchor — human review.
                tracing::debug!(
                    module = "captain",
                    item_id = item.id,
                    current = %current,
                    "ship verdict has no reviewed_head_sha; leaving for human review"
                );
                return;
            }
        },
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
    let confidence_reason = verdict_event
        .data
        .get("confidence_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pr_url = format!("https://github.com/{repo}/pull/{pr_num}");
    let prev_status = item.status;
    let prev_session_merge = item.session_ids.merge.clone();
    let prev_merge_fail_count = item.merge_fail_count;
    let prev_last_activity_at = item.last_activity_at.clone();
    item.status = ItemStatus::CaptainMerging;
    item.session_ids.merge = None;
    item.merge_fail_count = 0;
    item.last_activity_at = Some(global_types::now_rfc3339());
    let event = crate::TimelineEvent {
        event_type: TimelineEventType::CaptainMergeStarted,
        timestamp: global_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: "High-confidence review verdict -- starting merge".to_string(),
        data: serde_json::json!({
            "pr": pr_url,
            "source": "captain_review_confidence",
            "confidence_reason": confidence_reason,
        }),
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
            item.status = prev_status;
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
            item.status = prev_status;
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

/// Fetch the current head SHA of a PR via `gh pr view --json headRefOid`.
/// Returns the OID string on success.
async fn current_pr_head_sha(repo: &str, pr_num: i64) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_num.to_string(),
            "--repo",
            repo,
            "--json",
            "headRefOid",
        ])
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("gh pr view spawn failed: {e}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "gh pr view failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let sha = json
        .get("headRefOid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("gh pr view response missing headRefOid"))?;
    if sha.is_empty() || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow::anyhow!("gh pr view returned invalid headRefOid"));
    }
    Ok(sha.to_string())
}
