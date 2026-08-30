//! Parallel merge polling — checks GitHub merge status concurrently.

use crate::{ItemStatus, Task};
use settings::CaptainWorkflow;
use settings::Config;

use super::captain_merge::{
    apply_merge_result, check_merge, handle_merge_error, spawn_merge, MergeAttempt, MergeResult,
};
use super::notify::Notifier;
use crate::service::dispatch_logic;

/// Poll all CaptainMerging items — spawn sessions, check results, handle timeouts.
///
/// GitHub `is_pr_merged` checks run in parallel via `join_all`.
#[tracing::instrument(skip_all)]
pub(crate) async fn poll_merging_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    rate_limited: bool,
) {
    let merge_timeout = workflow.agent.captain_merge_timeout_s;
    let max_merge_retries = workflow.agent.max_merge_retries;
    let merge_cap = workflow
        .agent
        .per_state_limits
        .get("captain-merging")
        .copied();
    // Items already running a merge session occupy the cap. Items pending
    // a spawn (CaptainMerging without a session id) are candidates and
    // must not be counted, otherwise each candidate self-blocks.
    let mut merge_active = dispatch_logic::count_active_states(items)
        .get("captain-merging")
        .copied()
        .unwrap_or(0);

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
            .or_else(|| settings::resolve_github_repo(Some(&item.project), config));
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
            .map(|c| global_github::is_pr_merged(&c.repo, &c.pr_num))
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
                apply_merge_result(item, &result, notifier, workflow, pool).await;
                continue;
            }
            if let Some(cap) = merge_cap {
                if merge_active >= cap {
                    tracing::debug!(
                        module = "captain",
                        state = "captain-merging",
                        current = merge_active,
                        cap = cap,
                        title = %item.title,
                        "per-state cap reached — deferring dispatch"
                    );
                    continue;
                }
            }
            item.last_activity_at = Some(global_types::now_rfc3339());
            match spawn_merge(item, config, workflow, notifier, pool).await {
                Ok(MergeAttempt::Merged(result)) => {
                    apply_merge_result(item, &result, notifier, workflow, pool).await;
                }
                Ok(MergeAttempt::SessionSpawned) => {
                    merge_active += 1;
                }
                Err(e) => {
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
            // A merge session killed by a rate limit produces the same
            // synthetic failure as any other crash. Excuse it before
            // `handle_merge_error` charges it to the retry budget, exactly as
            // the review poller does for its own failed sessions — otherwise
            // a healthy PR escalates after `max_merge_retries` ticks of a
            // cooldown nobody's merge session could have survived.
            if result.action != "merged"
                && excuse_rate_limited_merge(item, pool, false, "merge session failed").await
            {
                continue;
            }
            apply_merge_result(item, &result, notifier, workflow, pool).await;
            continue;
        }

        // Stream file has no result. Before falling through to the timeout,
        // queue a GitHub API check to see if the PR was already merged.
        if let Some(pr_num) = item.pr_number {
            let repo = item
                .github_repo
                .clone()
                .or_else(|| settings::resolve_github_repo(Some(&item.project), config));
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
            .map(|(_, repo, pr_num)| global_github::is_pr_merged(repo, pr_num))
            .collect();
        let gh_results = futures::future::join_all(futs).await;

        for ((idx, _, _), gh_result) in pending_github_check.iter().zip(gh_results) {
            let item = &mut items[*idx];
            let already_merged = matches!(gh_result, Ok(true));

            if already_merged {
                let session_id = item.session_ids.merge.as_deref().unwrap_or("<none>");
                let stream_path = global_infra::paths::stream_path_for_session(session_id);
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
                apply_merge_result(item, &result, notifier, workflow, pool).await;
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
            if excuse_rate_limited_merge(item, pool, rate_limited, "merge timeout").await {
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

/// Clear a merge session that a rate limit killed, without charging it to the
/// retry budget. Returns true when the failure was excused, leaving the item
/// in CaptainMerging with no session id so the next tick re-spawns it once the
/// cooldown lifts.
///
/// `cooldown_active` excuses the item on the daemon-wide cooldown flag alone.
/// The timeout path sets it — a session that produced nothing while every
/// credential was cooling down learned nothing about this PR. A session that
/// failed outright has its own stream to read, so that path relies on the
/// stream check only.
async fn excuse_rate_limited_merge(
    item: &mut Task,
    pool: &sqlx::SqlitePool,
    cooldown_active: bool,
    what: &str,
) -> bool {
    let stream_says_rate_limited = match item.session_ids.merge.clone() {
        Some(sid) => super::credential_rate_limit::check_and_activate_from_stream(pool, &sid).await,
        None => false,
    };
    if !stream_says_rate_limited && !cooldown_active {
        return false;
    }
    tracing::info!(
        module = "captain",
        item_id = item.id,
        "{what} during rate limit — not counting against retry budget"
    );
    global_infra::best_effort!(
        super::timeline_emit::emit_rate_limited(item, pool).await,
        "captain_merge_poll: super::timeline_emit::emit_rate_limited(item, pool).await"
    );
    item.session_ids.merge = None;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> sqlx::SqlitePool {
        let db = global_db::Db::open_in_memory().await.unwrap();
        db.pool().clone()
    }

    /// Write a stream file carrying the `rejected` rate-limit envelope CC
    /// emits when the credential is exhausted, and register the session so
    /// the provider lookup resolves.
    async fn seed_rate_limited_session(
        pool: &sqlx::SqlitePool,
        data_dir: &std::path::Path,
    ) -> String {
        let session_id = global_infra::uuid::Uuid::v4().to_string();
        crate::io::headless_cc::log_running_session(
            pool,
            &session_id,
            std::path::Path::new("/tmp"),
            "merge-model",
            "captain-merge-async",
            "",
            Some(7),
            false,
            None,
        )
        .await
        .unwrap();
        let streams_dir = data_dir.join("state/cc-streams");
        std::fs::create_dir_all(&streams_dir).unwrap();
        std::fs::write(
            streams_dir.join(format!("{session_id}.jsonl")),
            "{\"type\":\"system\",\"subtype\":\"init\"}\n\
             {\"type\":\"rate_limit_event\",\"rate_limit_info\":{\"status\":\"rejected\"}}\n",
        )
        .unwrap();
        session_id
    }

    /// A merge session that failed because its credential was rate-limited
    /// must not burn the retry budget: three such ticks used to exhaust
    /// `max_merge_retries` and escalate a perfectly healthy PR.
    #[tokio::test]
    async fn a_rate_limited_merge_failure_does_not_burn_the_retry_budget() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let dir = std::env::temp_dir().join(format!(
            "mando-merge-rl-test-{}",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let _guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", &dir);
        super::super::ambient_rate_limit::clear();

        let pool = test_pool().await;
        let session_id = seed_rate_limited_session(&pool, &dir).await;

        let mut item = Task::new("Rate-limited merge");
        item.id = 7;
        item.status = ItemStatus::CaptainMerging;
        item.merge_fail_count = 1;
        item.session_ids.merge = Some(session_id);

        assert!(
            excuse_rate_limited_merge(&mut item, &pool, false, "merge session failed").await,
            "a rate-limited failure must be excused"
        );
        assert_eq!(
            item.merge_fail_count, 1,
            "an excused failure must not increment merge_fail_count"
        );
        assert!(
            item.session_ids.merge.is_none(),
            "the dead session must be cleared so the next tick re-spawns"
        );
        assert!(
            super::super::ambient_rate_limit::is_active(),
            "the cooldown must be active so nothing re-spawns until it lifts"
        );
    }

    /// A merge session that failed for any other reason still goes to
    /// `handle_merge_error` — this must not become a blanket retry excuse.
    #[tokio::test]
    async fn an_ordinary_merge_failure_is_not_excused() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let dir = std::env::temp_dir().join(format!(
            "mando-merge-ok-test-{}",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let _guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", &dir);
        super::super::ambient_rate_limit::clear();

        let pool = test_pool().await;
        let session_id = global_infra::uuid::Uuid::v4().to_string();
        crate::io::headless_cc::log_running_session(
            &pool,
            &session_id,
            std::path::Path::new("/tmp"),
            "merge-model",
            "captain-merge-async",
            "",
            Some(8),
            false,
            None,
        )
        .await
        .unwrap();
        let streams_dir = dir.join("state/cc-streams");
        std::fs::create_dir_all(&streams_dir).unwrap();
        std::fs::write(
            streams_dir.join(format!("{session_id}.jsonl")),
            "{\"type\":\"system\",\"subtype\":\"init\"}\n",
        )
        .unwrap();

        let mut item = Task::new("Plain merge failure");
        item.id = 8;
        item.status = ItemStatus::CaptainMerging;
        item.session_ids.merge = Some(session_id.clone());

        assert!(!excuse_rate_limited_merge(&mut item, &pool, false, "merge session failed").await);
        assert_eq!(item.session_ids.merge.as_deref(), Some(session_id.as_str()));
    }
}
