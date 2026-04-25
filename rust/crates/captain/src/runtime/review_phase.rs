//! Review phase — gather worker contexts and fetch PR data.

use crate::{Task, WorkerContext};
use anyhow::{anyhow, Result};
use settings::Config;

use crate::io::{health_store, pid_registry};
use crate::service::review_marshal;
use global_github as github;
use global_github as github_pr;

/// Compute seconds since a worker started from an RFC 3339 timestamp.
///
/// A missing timestamp is treated as "not started yet" and returns `Ok(0.0)`.
/// An unparseable timestamp is a persisted-state bug: it is propagated as an
/// error so the caller can abort instead of silently treating it as zero
/// seconds elapsed (which would bypass timeout rules).
fn seconds_since(started_at: Option<&str>) -> Result<f64> {
    let ts = match started_at {
        Some(s) => s,
        None => return Ok(0.0),
    };
    let started = time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339)
        .map_err(|e| anyhow!("unparseable worker_started_at '{ts}': {e}"))?;
    Ok((time::OffsetDateTime::now_utc() - started).as_seconds_f64())
}

/// Gather WorkerContext for each in-progress item with a worker.
///
/// Phase 1 (sync): collect local data (PID, stream, health) per item.
/// Phase 2 (parallel): run GitHub API calls (PR discovery + PR data fetch) concurrently.
/// Phase 3 (sync): apply discovered PRs back to items and assemble contexts.
#[tracing::instrument(skip_all)]
pub(crate) async fn gather_worker_contexts(
    items: &mut [Task],
    config: &Config,
    health_state: &health_store::HealthState,
    pool: &sqlx::SqlitePool,
) -> Result<Vec<WorkerContext>> {
    // Phase 1: collect sync-local data and build async work descriptors.
    let mut work: Vec<GatherWork> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        if item.status != crate::ItemStatus::InProgress || item.planning {
            continue;
        }
        let worker_name = match &item.worker {
            Some(w) => w.clone(),
            None => continue,
        };

        let stream_path = item
            .session_ids
            .worker
            .as_deref()
            .map(global_infra::paths::stream_path_for_session)
            .unwrap_or_default();

        let cc_sid = item.session_ids.worker.as_deref().unwrap_or("");
        let pid = pid_registry::get_pid(cc_sid).unwrap_or(crate::Pid::new(0));
        let process_alive = pid.as_u32() > 0 && global_claude::is_process_alive(pid);
        let stream_tail = crate::io::transcript::extract_stream_tail(&stream_path, 50);
        let stream_stale_s = global_claude::stream_stale_seconds(&stream_path);
        let prev_cpu_time_s =
            health_store::get_health_f64(health_state, worker_name.as_str(), "cpu_time_s");

        // Unparseable worker_started_at is a persisted-state bug: propagate
        // rather than bypass timeout rules with a silent 0.0.
        let seconds_active = seconds_since(item.worker_started_at.as_deref())?;

        // Build PR discovery args if needed.
        let discover_pr = if item.pr_number.is_none() {
            item.branch.as_deref().and_then(|branch| {
                let repo = settings::resolve_project_config(Some(&item.project), config)
                    .and_then(|(_, pc)| pc.github_repo.clone())?;
                Some((repo, branch.to_string()))
            })
        } else {
            None
        };

        // Resolve the GitHub repo slug from config for short PR ref resolution.
        let github_repo = settings::resolve_github_repo(Some(&item.project), config);

        work.push(GatherWork {
            item_idx: idx,
            worker_name,
            pid,
            process_alive,
            stream_tail,
            stream_stale_s,
            prev_cpu_time_s,
            seconds_active,
            discover_pr,
            existing_pr_number: item.pr_number,
            item_title: item.title.clone(),
            github_repo,
            branch: item.branch.clone(),
            intervention_count: item.intervention_count,
            no_pr: item.no_pr,
            reopen_seq: item.reopen_seq,
            reopen_source: item.reopen_source.clone(),
            task_id: item.id,
            reopened_at: item.reopened_at.clone(),
        });
    }

    if work.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 2: run GitHub API calls AND artifact queries in parallel.
    let futures: Vec<_> = work.iter().map(gather_one_async).collect();
    let artifact_futures: Vec<_> = work
        .iter()
        .map(|w| compute_artifact_gates(pool, w.task_id, w.reopen_seq, w.reopened_at.as_deref()))
        .collect();
    let (results, artifact_results) = tokio::join!(
        futures::future::join_all(futures),
        futures::future::join_all(artifact_futures),
    );

    // Phase 3: apply discovered PRs back and assemble contexts.
    let mut contexts = Vec::with_capacity(work.len());
    for ((w, result), artifact_gate) in work.iter().zip(results).zip(artifact_results) {
        // Write discovered PR number back to item.
        if let Some(pr_num) = result.discovered_pr_number {
            items[w.item_idx].pr_number = Some(pr_num);
        }

        let effective_pr_number = result.discovered_pr_number.or(w.existing_pr_number);
        let pr = effective_pr_number.map(|n| {
            w.github_repo
                .as_deref()
                .map(|repo| crate::pr_url(repo, n))
                .unwrap_or_else(|| crate::pr_label(n))
        });

        let has_reopen_ack = if w.reopen_seq > 0 {
            check_reopen_ack(
                &result.pr_data.body,
                &result.pr_data.issue_comment_bodies,
                w.reopen_seq,
            )
        } else {
            true
        };

        contexts.push(WorkerContext {
            session_name: w.worker_name.clone(),
            item_title: w.item_title.clone(),
            status: "in-progress".into(),
            branch: w.branch.clone(),
            pr,
            pr_ci_status: result.pr_data.ci_status,
            pr_comments: result.pr_data.comments,
            unresolved_threads: result.pr_data.unresolved_threads,
            unreplied_threads: result.pr_data.unreplied_threads,
            unaddressed_issue_comments: result.pr_data.unaddressed_issue_comments,
            pr_body: result.pr_data.body,
            changed_files: result.pr_data.changed_files,
            branch_ahead: result.pr_data.branch_ahead,
            process_alive: w.process_alive,
            cpu_time_s: result.cpu_time_s,
            prev_cpu_time_s: w.prev_cpu_time_s,
            stream_tail: w.stream_tail.clone(),
            seconds_active: w.seconds_active,
            intervention_count: w.intervention_count,
            no_pr: w.no_pr,
            reopen_seq: w.reopen_seq,
            has_reopen_ack,
            reopen_source: w.reopen_source.clone(),
            stream_stale_s: w.stream_stale_s,
            pr_head_sha: result.pr_data.head_sha,
            degraded: result.pr_data.degraded,
            has_evidence: artifact_gate.has_evidence,
            evidence_fresh: artifact_gate.evidence_fresh,
            has_work_summary: artifact_gate.has_work_summary,
            work_summary_fresh: artifact_gate.work_summary_fresh,
            has_screenshot: artifact_gate.has_screenshot,
            has_recording: artifact_gate.has_recording,
        });
    }

    Ok(contexts)
}

use super::review_phase_artifacts::compute_artifact_gates;

/// Sync-local data collected in phase 1 for each worker.
struct GatherWork {
    item_idx: usize,
    worker_name: String,
    pid: crate::Pid,
    process_alive: bool,
    stream_tail: String,
    stream_stale_s: Option<f64>,
    prev_cpu_time_s: Option<f64>,
    seconds_active: f64,
    /// (repo, branch) if PR discovery is needed.
    discover_pr: Option<(String, String)>,
    existing_pr_number: Option<i64>,
    item_title: String,
    github_repo: Option<String>,
    branch: Option<String>,
    intervention_count: i64,
    no_pr: bool,
    reopen_seq: i64,
    reopen_source: Option<String>,
    task_id: i64,
    reopened_at: Option<String>,
}

/// Async results from phase 2 for each worker.
struct GatherResult {
    discovered_pr_number: Option<i64>,
    pr_data: PrData,
    cpu_time_s: Option<f64>,
}

/// Run the async portion of context gathering for one worker (PR discovery + PR data + CPU time).
async fn gather_one_async(w: &GatherWork) -> GatherResult {
    // CPU time (async syscall).
    let cpu_time_s = if w.process_alive && w.pid.as_u32() > 0 {
        global_claude::get_cpu_time(w.pid).await.ok()
    } else {
        None
    };

    // PR discovery.
    let discovered_pr_number = if let Some((ref repo, ref branch)) = w.discover_pr {
        let pr_num = github::discover_pr_for_branch(repo, branch).await;
        if let Some(num) = pr_num {
            tracing::info!(
                module = "captain",
                worker = %w.worker_name,
                pr = num,
                "discovered PR for branch"
            );
        }
        pr_num
    } else {
        None
    };

    // Build a minimal Task to pass to fetch_pr_data.
    let effective_pr_number = discovered_pr_number.or(w.existing_pr_number);
    let stub = Task {
        pr_number: effective_pr_number,
        github_repo: w.github_repo.clone(),
        ..Task::new("")
    };
    let pr_data = fetch_pr_data(&stub).await;

    GatherResult {
        discovered_pr_number,
        pr_data,
        cpu_time_s,
    }
}

/// PR data fetched from GitHub, with a degraded flag when data is partial.
#[derive(Default)]
pub struct PrData {
    pub ci_status: Option<String>,
    pub comments: i64,
    pub unresolved_threads: i64,
    pub unreplied_threads: i64,
    pub unaddressed_issue_comments: i64,
    pub body: String,
    pub changed_files: Vec<String>,
    pub branch_ahead: bool,
    pub head_sha: String,
    pub issue_comment_bodies: Vec<String>,
    pub degraded: bool,
}

/// Fetch PR data from GitHub. Returns default values on failure (non-fatal).
#[tracing::instrument(skip_all)]
pub(crate) async fn fetch_pr_data(item: &Task) -> PrData {
    let pr_num = match item.pr_number {
        Some(n) => n,
        None => return PrData::default(),
    };

    // Resolve repo slug from github_repo (populated via JOIN) or project config.
    let repo = match &item.github_repo {
        Some(r) if !r.is_empty() => r.clone(),
        _ => {
            if !item.project.is_empty() {
                item.project.clone()
            } else {
                tracing::warn!(
                    module = "captain",
                    pr_number = pr_num,
                    "PR number with no github_repo — cannot fetch PR data"
                );
                return PrData::default();
            }
        }
    };
    let pr_number_str = pr_num.to_string();

    match github::fetch_pr_status(&repo, &pr_number_str).await {
        Ok(status) => {
            let mut degraded = false;

            let ahead = github::is_pr_branch_ahead(&repo, &pr_number_str)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(module = "captain", pr_number = pr_num, error = %e, "branch-ahead check failed");
                    degraded = true;
                    false
                });

            let pr_num_u32: u32 = pr_number_str.parse().unwrap_or(0);

            // Compute PR hygiene from structured thread/comment data.
            let mut comment_bodies: Vec<String> = Vec::new();
            let (hygiene_unresolved, hygiene_unreplied, unaddressed) = if pr_num_u32 > 0 {
                let thread_counts = match github_pr::get_pr_review_threads(&repo, pr_num_u32).await
                {
                    Ok(threads) => review_marshal::thread_hygiene(&threads, &status.author),
                    Err(e) => {
                        tracing::warn!(module = "captain", pr = pr_num_u32, error = %e, "GraphQL review threads fetch failed, falling back to zeros");
                        degraded = true;
                        (status.unresolved_threads, status.unreplied_threads)
                    }
                };
                let issue_count = if status.comments > 0 {
                    match github_pr::get_pr_comments(&repo, pr_num_u32).await {
                        Ok(comments) => {
                            comment_bodies = comments.iter().map(|c| c.body.clone()).collect();
                            review_marshal::issue_comment_hygiene(&comments, &status.author)
                        }
                        Err(e) => {
                            tracing::warn!(module = "captain", pr = pr_num_u32, error = %e, "issue comments fetch failed, falling back to zero");
                            degraded = true;
                            0
                        }
                    }
                } else {
                    0
                };
                (thread_counts.0, thread_counts.1, issue_count)
            } else {
                (status.unresolved_threads, status.unreplied_threads, 0)
            };

            PrData {
                ci_status: status.ci_status,
                comments: status.comments,
                unresolved_threads: hygiene_unresolved,
                unreplied_threads: hygiene_unreplied,
                unaddressed_issue_comments: unaddressed,
                body: status.body,
                changed_files: status.changed_files,
                branch_ahead: ahead,
                head_sha: status.head_sha,
                issue_comment_bodies: comment_bodies,
                degraded,
            }
        }
        Err(e) => {
            tracing::warn!(module = "captain", pr_number = pr_num, error = %e, "failed to fetch PR status");
            PrData {
                degraded: true,
                ..PrData::default()
            }
        }
    }
}

/// Check if PR body or issue comments contain a reopen-ack marker for the given sequence.
///
/// Matches worker-posted comment: `[Mando] ... Reopen #{seq} addressed:` (case-insensitive)
fn check_reopen_ack(body: &str, issue_comments: &[String], reopen_seq: i64) -> bool {
    let comment_marker = format!("reopen #{} addressed", reopen_seq);

    for comment in std::iter::once(body).chain(issue_comments.iter().map(|s| s.as_str())) {
        if comment
            .to_lowercase()
            .contains(&comment_marker.to_lowercase())
        {
            return true;
        }
    }
    false
}

/// Build a single WorkerContext for an item (used by async captain review).
#[tracing::instrument(skip_all)]
pub(crate) async fn build_single_context(
    item: &Task,
    config: &settings::Config,
) -> Result<(crate::WorkerContext, String)> {
    use crate::service::worker_context;

    let worker_name = item.worker.as_deref().unwrap_or("unknown");
    let stream_path = item
        .session_ids
        .worker
        .as_deref()
        .map(global_infra::paths::stream_path_for_session)
        .unwrap_or_default();
    let stream_tail = crate::io::transcript::extract_stream_tail(&stream_path, 50);
    let stream_stale_s = global_claude::stream_stale_seconds(&stream_path);

    let seconds_active = seconds_since(item.worker_started_at.as_deref())?;

    // Resolve github repo slug for PR data fetch.
    let github_repo = settings::resolve_github_repo(Some(&item.project), config);
    let stub = Task {
        pr_number: item.pr_number,
        github_repo: github_repo.clone(),
        ..Task::new("")
    };
    let pr_data = fetch_pr_data(&stub).await;

    let has_reopen_ack = if item.reopen_seq > 0 {
        check_reopen_ack(
            &pr_data.body,
            &pr_data.issue_comment_bodies,
            item.reopen_seq,
        )
    } else {
        true
    };

    let pr_str = item.pr_number.map(|n| {
        github_repo
            .as_deref()
            .map(|repo| crate::pr_url(repo, n))
            .unwrap_or_else(|| crate::pr_label(n))
    });
    let ctx = WorkerContext {
        session_name: worker_name.to_string(),
        item_title: item.title.clone(),
        status: "in-progress".into(),
        branch: item.branch.clone(),
        pr: pr_str,
        pr_ci_status: pr_data.ci_status,
        pr_comments: pr_data.comments,
        unresolved_threads: pr_data.unresolved_threads,
        unreplied_threads: pr_data.unreplied_threads,
        unaddressed_issue_comments: pr_data.unaddressed_issue_comments,
        pr_body: pr_data.body,
        changed_files: pr_data.changed_files,
        branch_ahead: pr_data.branch_ahead,
        // process_alive and cpu_time require health_state (PID tracking), which
        // is not available in the async review path. The trigger context already
        // encodes whether the worker was alive.
        process_alive: false,
        cpu_time_s: None,
        prev_cpu_time_s: None,
        stream_tail,
        seconds_active,
        intervention_count: item.intervention_count,
        no_pr: item.no_pr,
        reopen_seq: item.reopen_seq,
        has_reopen_ack,
        reopen_source: item.reopen_source.clone(),
        stream_stale_s,
        pr_head_sha: pr_data.head_sha,
        degraded: pr_data.degraded,
        // Artifact gates are not needed for captain review context
        // (review uses evidence files directly, not gate booleans).
        has_evidence: false,
        evidence_fresh: false,
        has_work_summary: false,
        work_summary_fresh: false,
        has_screenshot: false,
        has_recording: false,
    };
    let formatted = worker_context::format_context(&ctx);
    Ok((ctx, formatted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_marker_no_longer_matches() {
        assert!(!check_reopen_ack("reopen-ack:1", &[], 1));
    }

    #[test]
    fn worker_comment_format() {
        let comments = vec!["[Mando] Review-Reopen #1 addressed: extracted act.rs".into()];
        assert!(check_reopen_ack("", &comments, 1));
    }

    #[test]
    fn case_insensitive_match() {
        let comments = vec!["[mando] reopen #2 addressed: fix".into()];
        assert!(check_reopen_ack("", &comments, 2));
    }

    #[test]
    fn no_match() {
        let comments = vec!["Some unrelated comment".into()];
        assert!(!check_reopen_ack("no marker here", &comments, 1));
    }

    #[test]
    fn marker_in_body_with_comment_format() {
        assert!(check_reopen_ack(
            "body with Reopen #3 addressed: stuff",
            &[],
            3,
        ));
    }

    #[test]
    fn wrong_seq_no_match() {
        let comments = vec!["[Mando] Reopen #1 addressed: fix".into()];
        assert!(!check_reopen_ack("", &comments, 2));
    }
}
