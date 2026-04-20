//! Mergeability phase — check merge conflicts, review threads, CI failures.

use crate::Task;
use anyhow::Result;
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

use crate::io::health_store::HealthState;
use crate::runtime::mergeability_rebase::{
    check_pr_mergeable, handle_conflict, reap_dead_rebase_workers, MergeStatus,
};
use crate::runtime::notify::Notifier;
use crate::service::{lifecycle, merge_logic};

/// Check pending-review items for merge conflicts.
///
/// For items with PRs: check mergeable status via `gh pr view`.
/// Spawn rebase workers for conflicted items.
#[tracing::instrument(skip_all)]
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
                (it.status == crate::ItemStatus::AwaitingReview
                    || it.status == crate::ItemStatus::HandedOff)
                    && it.pr_number.is_none()
                    && it.branch.is_some()
            })
            .filter_map(|(i, it)| {
                let branch = it.branch.clone()?;
                let repo = it
                    .github_repo
                    .clone()
                    .or_else(|| settings::config::resolve_github_repo(Some(&it.project), config))?;
                Some((i, repo, branch))
            })
            .collect();

        if !discover_jobs.is_empty() {
            let futs: Vec<_> = discover_jobs
                .iter()
                .map(|(_, repo, branch)| crate::io::github::discover_pr_for_branch(repo, branch))
                .collect();
            let results = futures::future::join_all(futs).await;

            for ((idx, _, _), pr_num) in discover_jobs.iter().zip(results) {
                if let Some(num) = pr_num {
                    tracing::info!(
                        module = "captain",
                        title = %items[*idx].title,
                        pr_number = num,
                        "discovered PR in mergeability phase"
                    );
                    items[*idx].pr_number = Some(num);
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
            let pr_num = item.pr_number?;
            let repo = item
                .github_repo
                .clone()
                .or_else(|| settings::config::resolve_github_repo(Some(&item.project), config))
                .or_else(|| {
                    tracing::debug!(
                        module = "captain",
                        title = %item.title,
                        project = %item.project,
                        "skipping mergeability check — no github_repo"
                    );
                    None
                })?;
            let pr = crate::pr_url(&repo, pr_num);
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
                if config.captain.auto_merge {
                    super::mergeability_auto_merge::try_auto_merge_from_verdict(
                        &mut items[idx],
                        config,
                        notifier,
                        alerts,
                        pool,
                    )
                    .await;
                } else {
                    tracing::debug!(module = "captain", pr = %pr, "PR is mergeable, awaiting human");
                }
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
            let pr_num = item.pr_number?;
            let repo = item
                .github_repo
                .clone()
                .or_else(|| settings::config::resolve_github_repo(Some(&item.project), config))?;
            let pr = crate::pr_url(&repo, pr_num);
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
    new_status: crate::ItemStatus,
    data: crate::TimelineEventPayload,
    emoji: &str,
    verb_present: &str,
    verb_past: &str,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let prev_status = item.status;
    if let Err(e) = lifecycle::apply_transition(item, new_status) {
        tracing::error!(
            module = "captain",
            item_id = item.id,
            error = %e,
            "illegal terminal GitHub transition"
        );
        return;
    }
    let event = crate::TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!("PR {pr} {verb_past} on GitHub"),
        data,
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
            let msg = format!(
                "{} {} (PR {}): <b>{}</b>",
                emoji,
                verb_present,
                pr,
                global_infra::html::escape_html(&item.title)
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
            lifecycle::restore_status(item, prev_status);
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
        crate::ItemStatus::Merged,
        crate::TimelineEventPayload::Merged {
            pr: pr.to_string(),
            source: "github".to_string(),
            accepted_by: "github".to_string(),
        },
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
        crate::ItemStatus::Canceled,
        crate::TimelineEventPayload::Canceled { pr: pr.to_string() },
        "\u{26d4}",
        "PR closed",
        "closed",
        notifier,
        pool,
    )
    .await;
}
