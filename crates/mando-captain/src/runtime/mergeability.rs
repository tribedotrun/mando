//! Mergeability phase — check merge conflicts, review threads, CI failures.

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::Task;

use crate::biz::merge_logic;
use crate::io::health_store::HealthState;
use crate::runtime::mergeability_rebase::{
    check_pr_mergeable, handle_conflict, reap_dead_rebase_workers, MergeStatus,
};
use crate::runtime::notify::Notifier;

/// Check pending-review items for merge conflicts.
///
/// For items with PRs: check mergeable status via `gh pr view`.
/// Spawn rebase workers for conflicted items.
pub(crate) async fn check_done_mergeability(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    alerts: &mut Vec<String>,
    _health_state: &HealthState,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    // Discover PRs for pending-review and handed-off items — parallel discovery.
    {
        let discover_jobs: Vec<(usize, String, String)> = items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                (it.status == mando_types::task::ItemStatus::AwaitingReview
                    || it.status == mando_types::task::ItemStatus::HandedOff)
                    && it.pr.is_none()
                    && it.branch.is_some()
            })
            .filter_map(|(i, it)| {
                let branch = it.branch.clone()?;
                let repo = mando_config::resolve_github_repo(it.project.as_deref(), config)?;
                Some((i, repo, branch))
            })
            .collect();

        if !discover_jobs.is_empty() {
            let futs: Vec<_> = discover_jobs
                .iter()
                .map(|(_, repo, branch)| crate::io::github::discover_pr_for_branch(repo, branch))
                .collect();
            let results = futures::future::join_all(futs).await;

            for ((idx, _, _), pr_url) in discover_jobs.iter().zip(results) {
                if let Some(url) = pr_url {
                    tracing::info!(
                        module = "captain",
                        title = %items[*idx].title,
                        pr = %url,
                        "discovered PR in mergeability phase"
                    );
                    items[*idx].pr = Some(url);
                }
            }
        }
    }

    // Reap dead rebase workers. If the process exited, detect whether it
    // succeeded (SHA changed) before clearing `rebase_worker`.
    reap_dead_rebase_workers(items, pool).await;

    let candidates = merge_logic::items_needing_rebase_check(items);

    // Collect (idx, pr, repo) for all candidates, then check mergeability in parallel.
    let candidate_checks: Vec<(usize, String, String)> = candidates
        .iter()
        .filter_map(|&idx| {
            let item = &items[idx];
            let pr = item.pr.clone()?;
            let repo = mando_config::resolve_github_repo(item.project.as_deref(), config).or_else(
                || {
                    tracing::debug!(
                        module = "captain",
                        title = %item.title,
                        project = ?item.project,
                        "skipping mergeability check — no github_repo configured"
                    );
                    None
                },
            )?;
            Some((idx, pr, repo))
        })
        .collect();

    // Run candidate mergeability checks in parallel.
    let candidate_futures: Vec<_> = candidate_checks
        .iter()
        .map(|(_, pr, repo)| check_pr_mergeable(pr, repo))
        .collect();
    let candidate_results = futures::future::join_all(candidate_futures).await;

    // Apply candidate mergeability results sequentially (mutations, notifications).
    for ((idx, pr, _repo), result) in candidate_checks.iter().zip(candidate_results) {
        let idx = *idx;
        match result {
            Ok(MergeStatus::Merged) => {
                apply_merged(&mut items[idx], pr, config, notifier, pool).await;
            }
            Ok(MergeStatus::Closed) => {
                apply_closed(&mut items[idx], pr, config, notifier, pool).await;
            }
            Ok(MergeStatus::Mergeable) => {
                tracing::debug!(module = "captain", pr = %pr, "PR is mergeable, awaiting human");
            }
            Ok(MergeStatus::Conflicted) => {
                handle_conflict(items, idx, pr, config, workflow, notifier, alerts, pool).await;
            }
            Ok(MergeStatus::Unknown) => {
                tracing::debug!(module = "captain", pr = %pr, "mergeability check inconclusive");
            }
            Err(e) => {
                tracing::warn!(module = "captain", pr = %pr, error = %e, "mergeability check failed");
            }
        }
    }

    // Compute watch list AFTER candidate mutations are applied.
    let merge_watch = merge_logic::items_needing_merge_watch(items);
    let watch_checks: Vec<(usize, String, String)> = merge_watch
        .iter()
        .filter_map(|&idx| {
            let item = &items[idx];
            let pr = item.pr.clone()?;
            let repo = mando_config::resolve_github_repo(item.project.as_deref(), config)?;
            Some((idx, pr, repo))
        })
        .collect();

    let watch_futures: Vec<_> = watch_checks
        .iter()
        .map(|(_, pr, repo)| check_pr_mergeable(pr, repo))
        .collect();
    let watch_results = futures::future::join_all(watch_futures).await;

    // Apply merge-watch results sequentially.
    for ((idx, pr, _repo), result) in watch_checks.iter().zip(watch_results) {
        let idx = *idx;
        match result {
            Ok(MergeStatus::Merged) => {
                apply_merged(&mut items[idx], pr, config, notifier, pool).await;
            }
            Ok(MergeStatus::Closed) => {
                apply_closed(&mut items[idx], pr, config, notifier, pool).await;
            }
            Ok(_) => {} // Mergeable, Conflicted, Unknown — human owns it, no action
            Err(e) => {
                tracing::warn!(
                    module = "captain",
                    pr = %pr,
                    error = %e,
                    "merge-watch check failed for handed-off item"
                );
            }
        }
    }

    // Check for failed rebase workers.
    for item in items.iter() {
        if merge_logic::is_rebase_failed(item) {
            alerts.push(format!(
                "Rebase failed for '{}' — may need manual intervention",
                item.title
            ));
        }
    }

    // Check pending-review items for unaddressed review comments and CI failures.
    super::mergeability_review::check_done_review_threads(
        items, config, workflow, notifier, alerts, pool,
    )
    .await;

    Ok(())
}

/// Apply a terminal PR status (merged or closed) discovered on GitHub.
/// Sets the item status, notifies, and emits a timeline event; factoring out
/// the duplicated scaffolding between `apply_merged` and `apply_closed`.
#[allow(clippy::too_many_arguments)]
async fn apply_terminal_from_github(
    item: &mut Task,
    pr: &str,
    new_status: mando_types::task::ItemStatus,
    event_type: mando_types::timeline::TimelineEventType,
    emoji: &str,
    verb_present: &str,
    verb_past: &str,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let prev_status = item.status;
    item.status = new_status;
    let event = mando_types::timeline::TimelineEvent {
        event_type,
        timestamp: mando_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!("PR {pr} {verb_past} on GitHub"),
        data: serde_json::json!({ "pr": pr }),
    };
    match mando_db::queries::tasks::persist_status_transition(
        pool,
        item,
        prev_status.as_str(),
        &event,
    )
    .await
    {
        Ok(true) => {
            let msg = format!(
                "{} {} (PR {}): <b>{}</b>",
                emoji,
                verb_present,
                pr,
                mando_shared::telegram_format::escape_html(&item.title)
            );
            notifier.high(&msg).await;
            tracing::info!(
                module = "captain",
                title = %item.title,
                pr = %pr,
                verb_past = verb_past,
                "item {}",
                verb_past
            );
        }
        Ok(false) => {
            tracing::info!(
                module = "captain",
                item_id = item.id,
                "terminal from GitHub already applied"
            );
        }
        Err(e) => {
            item.status = prev_status;
            tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for terminal from GitHub");
        }
    }
}

async fn apply_merged(
    item: &mut Task,
    pr: &str,
    _config: &Config,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    apply_terminal_from_github(
        item,
        pr,
        mando_types::task::ItemStatus::Merged,
        mando_types::timeline::TimelineEventType::Merged,
        "\u{1f389}",
        "Merged",
        "merged",
        notifier,
        pool,
    )
    .await;
}

async fn apply_closed(
    item: &mut Task,
    pr: &str,
    _config: &Config,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    apply_terminal_from_github(
        item,
        pr,
        mando_types::task::ItemStatus::Canceled,
        mando_types::timeline::TimelineEventType::Canceled,
        "\u{26d4}",
        "PR closed",
        "closed",
        notifier,
        pool,
    )
    .await;
}
