//! Parallel merge polling — checks GitHub merge status concurrently.

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};

use super::captain_merge::{
    apply_merge_result, check_merge, handle_merge_error, spawn_merge, MergeResult,
};
use super::notify::Notifier;

/// Poll all CaptainMerging items — spawn sessions, check results, handle timeouts.
///
/// GitHub `is_pr_merged` checks run in parallel via `join_all`.
pub(crate) async fn poll_merging_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    rate_limited: bool,
) {
    let merge_timeout_s = workflow.agent.captain_merge_timeout_s;
    let max_merge_retries = workflow.agent.max_review_retries;

    // Categorize CaptainMerging items by state.
    let mut needs_spawn: Vec<usize> = Vec::new();
    let mut has_session: Vec<usize> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        if item.status != ItemStatus::CaptainMerging {
            continue;
        }
        let has_sid = item
            .session_ids
            .merge
            .as_deref()
            .is_some_and(|s| !s.is_empty());
        if has_sid {
            has_session.push(idx);
        } else if !rate_limited {
            needs_spawn.push(idx);
        } else {
            tracing::debug!(
                module = "captain",
                item_id = item.id,
                "skipping merge spawn during rate-limit cooldown"
            );
        }
    }

    // Phase 1: Check if PRs are already merged (parallel GitHub API calls).
    struct MergeCheck {
        idx: usize,
        repo: String,
        pr_num: String,
    }
    let mut checks: Vec<MergeCheck> = Vec::new();
    let mut spawn_direct: Vec<usize> = Vec::new();

    for &idx in &needs_spawn {
        let item = &items[idx];
        let mut matched = false;
        if let Some(pr_ref) = item.pr.as_deref() {
            let repo = mando_config::resolve_github_repo(item.project.as_deref(), config)
                .unwrap_or_default();
            let pr_num = mando_types::task::extract_pr_number(pr_ref)
                .map(|n| n.to_string())
                .unwrap_or_default();
            if !repo.is_empty() && !pr_num.is_empty() {
                checks.push(MergeCheck { idx, repo, pr_num });
                matched = true;
            }
        }
        if !matched {
            spawn_direct.push(idx);
        }
    }

    // Run is_pr_merged checks in parallel.
    if !checks.is_empty() {
        let futs: Vec<_> = checks
            .iter()
            .map(|c| crate::io::github::is_pr_merged(&c.repo, &c.pr_num))
            .collect();
        let merge_results = futures::future::join_all(futs).await;

        for (check, already_merged) in checks.iter().zip(merge_results) {
            let item = &mut items[check.idx];
            if already_merged {
                let result = MergeResult {
                    action: "merged".into(),
                    feedback: "PR already merged on GitHub — skipped merge session".into(),
                };
                apply_merge_result(item, &result, notifier, config, pool).await;
            } else {
                item.last_activity_at = Some(mando_types::now_rfc3339());
                if let Err(e) = spawn_merge(item, config, workflow, notifier, pool).await {
                    tracing::warn!(module = "captain", item_id = item.id, error = %e, "spawn_merge failed");
                    handle_merge_error(
                        item,
                        &format!("spawn failed: {e}"),
                        max_merge_retries,
                        notifier,
                        pool,
                    )
                    .await;
                }
            }
        }
    }

    // Items without PR info → spawn directly.
    for &idx in &spawn_direct {
        let item = &mut items[idx];
        item.last_activity_at = Some(mando_types::now_rfc3339());
        if let Err(e) = spawn_merge(item, config, workflow, notifier, pool).await {
            tracing::warn!(module = "captain", item_id = item.id, error = %e, "spawn_merge failed");
            handle_merge_error(
                item,
                &format!("spawn failed: {e}"),
                max_merge_retries,
                notifier,
                pool,
            )
            .await;
        }
    }

    // Phase 2: Poll items with existing sessions.
    for &idx in &has_session {
        let item = &mut items[idx];
        if let Some(result) = check_merge(item) {
            apply_merge_result(item, &result, notifier, config, pool).await;
        } else {
            let is_timed_out = item
                .last_activity_at
                .as_deref()
                .and_then(|ts| {
                    time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339)
                        .ok()
                })
                .map(|entered| {
                    let elapsed = time::OffsetDateTime::now_utc() - entered;
                    elapsed.whole_seconds() as u64 > merge_timeout_s
                })
                .unwrap_or(true);

            if is_timed_out {
                let is_rl = item.session_ids.merge.as_deref().is_some_and(|sid| {
                    super::rate_limit_cooldown::check_and_activate_from_stream(sid)
                });
                if is_rl || rate_limited {
                    tracing::info!(
                        module = "captain",
                        item_id = item.id,
                        "merge timeout during rate limit — not counting against retry budget"
                    );
                    super::timeline_emit::emit_rate_limited(item, pool).await;
                    item.session_ids.merge = None;
                    continue;
                }

                handle_merge_error(
                    item,
                    "merge session timed out without producing a result",
                    max_merge_retries,
                    notifier,
                    pool,
                )
                .await;
            }
        }
    }
}
