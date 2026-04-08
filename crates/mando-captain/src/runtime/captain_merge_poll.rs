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
    let merge_timeout = workflow.agent.captain_merge_timeout_s;
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

    // Items whose project/PR context is broken: escalate to captain review
    // instead of spawning a merge session against a malformed context that
    // would burn tokens and possibly hallucinate a wrong-PR merge.
    let mut escalate_unresolved: Vec<(usize, String)> = Vec::new();
    for &idx in &needs_spawn {
        let item = &items[idx];
        let Some(pr_num) = item.pr_number else {
            escalate_unresolved.push((idx, "item has no PR number".to_string()));
            continue;
        };
        let repo = item
            .github_repo
            .clone()
            .or_else(|| mando_config::resolve_github_repo(Some(&item.project), config));
        match repo {
            Some(repo) if !repo.is_empty() => {
                checks.push(MergeCheck {
                    idx,
                    repo,
                    pr_num: pr_num.to_string(),
                });
            }
            None => escalate_unresolved.push((
                idx,
                format!("cannot resolve github_repo for project {:?}", item.project),
            )),
            Some(_) => {
                escalate_unresolved.push((idx, "resolved github_repo is empty string".to_string()))
            }
        }
    }
    // Push the escalations forward so they are handled in the normal flow.
    for (idx, reason) in &escalate_unresolved {
        let item = &mut items[*idx];
        tracing::error!(
            module = "captain",
            item_id = item.id,
            reason = %reason,
            "captain-merging item has broken PR/project context, escalating to captain review"
        );
        handle_merge_error(
            item,
            &format!("config/data mismatch: {reason}"),
            max_merge_retries,
            notifier,
            pool,
        )
        .await;
    }

    // Run is_pr_merged checks in parallel.
    if !checks.is_empty() {
        let futs: Vec<_> = checks
            .iter()
            .map(|c| crate::io::github::is_pr_merged(&c.repo, &c.pr_num))
            .collect();
        let merge_results = futures::future::join_all(futs).await;

        for (check, merge_result) in checks.iter().zip(merge_results) {
            let item = &mut items[check.idx];
            let already_merged = match merge_result {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(
                        module = "captain",
                        item_id = item.id,
                        error = %e,
                        "is_pr_merged check failed; treating as not merged and spawning"
                    );
                    false
                }
            };
            if already_merged {
                let result = MergeResult {
                    action: "merged".into(),
                    feedback: "PR already merged on GitHub; skipped merge session".into(),
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

    // (All unresolved/broken-context items are now escalated above via
    // handle_merge_error rather than spawned with a malformed context.)

    // Phase 2: Poll items with existing sessions.
    //
    // For items where the stream file has no result yet, also check GitHub as a
    // fallback — the PR may have been merged successfully even when the stream
    // file is empty (e.g. CC stdout was never captured due to pipe/buffering
    // issues). This avoids waiting for the full merge timeout.
    let mut pending_github_check: Vec<(usize, String, String)> = Vec::new();
    let mut pending_timeout_only: Vec<usize> = Vec::new();

    for &idx in &has_session {
        let item = &mut items[idx];
        if let Some(result) = check_merge(item) {
            apply_merge_result(item, &result, notifier, config, pool).await;
            continue;
        }

        // Stream file has no result. Before falling through to the timeout,
        // queue a GitHub API check to see if the PR was already merged.
        if let Some(pr_num) = item.pr_number {
            let repo = item
                .github_repo
                .clone()
                .or_else(|| mando_config::resolve_github_repo(Some(&item.project), config));
            if let Some(repo) = repo {
                if !repo.is_empty() {
                    pending_github_check.push((idx, repo, pr_num.to_string()));
                    continue;
                }
            }
        }
        // No valid PR/repo for GitHub check — still needs timeout handling.
        pending_timeout_only.push(idx);
    }

    // Run GitHub is_pr_merged checks in parallel for items with no stream result.
    // Items confirmed merged skip timeout; items not merged fall through to timeout.
    let mut needs_timeout: Vec<usize> = pending_timeout_only;

    if !pending_github_check.is_empty() {
        let futs: Vec<_> = pending_github_check
            .iter()
            .map(|(_, repo, pr_num)| crate::io::github::is_pr_merged(repo, pr_num))
            .collect();
        let gh_results = futures::future::join_all(futs).await;

        for ((idx, _, _), gh_result) in pending_github_check.iter().zip(gh_results) {
            let item = &mut items[*idx];
            let already_merged = matches!(gh_result, Ok(true));

            if already_merged {
                let session_id = item.session_ids.merge.as_deref().unwrap_or("<none>");
                let stream_path = mando_config::stream_path_for_session(session_id);
                let stream_size = std::fs::metadata(&stream_path)
                    .map(|m| m.len())
                    .unwrap_or(u64::MAX);
                tracing::warn!(
                    module = "captain",
                    item_id = item.id,
                    session_id,
                    stream_file_bytes = stream_size,
                    "merge poll: PR already merged on GitHub but stream file had no result — recovering via GitHub fallback"
                );
                let result = MergeResult {
                    action: "merged".into(),
                    feedback: "PR already merged on GitHub; stream file had no result".into(),
                };
                apply_merge_result(item, &result, notifier, config, pool).await;
            } else {
                needs_timeout.push(*idx);
            }
        }
    }

    // Timeout handling for all items that had no stream result and weren't
    // already merged on GitHub.
    for idx in needs_timeout {
        let item = &mut items[idx];
        let is_timed_out = match item.last_activity_at.as_deref() {
            Some(ts) => match time::OffsetDateTime::parse(
                ts,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(entered) => {
                    let elapsed = time::OffsetDateTime::now_utc() - entered;
                    elapsed.whole_seconds() as u64 > merge_timeout.as_secs()
                }
                Err(e) => {
                    tracing::warn!(
                        module = "captain",
                        item_id = item.id,
                        last_activity_at = %ts,
                        error = %e,
                        "unparseable last_activity_at on captain-merging item; skipping this tick"
                    );
                    continue;
                }
            },
            None => {
                tracing::warn!(
                    module = "captain",
                    item_id = item.id,
                    "captain-merging item has no last_activity_at; skipping this tick"
                );
                continue;
            }
        };

        if is_timed_out {
            let is_rl =
                item.session_ids.merge.as_deref().is_some_and(|sid| {
                    super::rate_limit_cooldown::check_and_activate_from_stream(sid)
                });
            if is_rl || rate_limited {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    "merge timeout during rate limit — not counting against retry budget"
                );
                let _ = super::timeline_emit::emit_rate_limited(item, pool).await;
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
