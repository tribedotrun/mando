//! Review phase — gather worker contexts and fetch PR data.

use mando_config::settings::Config;
use mando_types::Task;
use mando_types::WorkerContext;

use crate::biz::review_marshal;
use crate::io::{github, github_pr, health_store, pid_registry};

/// Compute seconds since a worker started from an RFC 3339 timestamp.
fn seconds_since(started_at: Option<&str>) -> f64 {
    started_at
        .and_then(|ts| {
            time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339)
                .map_err(|e| tracing::warn!(module = "captain", timestamp = %ts, error = %e, "unparseable started_at"))
                .ok()
        })
        .map(|started| (time::OffsetDateTime::now_utc() - started).as_seconds_f64())
        .unwrap_or(0.0)
}

/// Gather WorkerContext for each in-progress item with a worker.
///
/// Phase 1 (sync): collect local data (PID, stream, health) per item.
/// Phase 2 (parallel): run GitHub API calls (PR discovery + PR data fetch) concurrently.
/// Phase 3 (sync): apply discovered PRs back to items and assemble contexts.
pub(crate) async fn gather_worker_contexts(
    items: &mut [Task],
    config: &Config,
    health_state: &health_store::HealthState,
) -> Vec<WorkerContext> {
    // Phase 1: collect sync-local data and build async work descriptors.
    let mut work: Vec<GatherWork> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        if item.status != mando_types::task::ItemStatus::InProgress {
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
            .map(mando_config::stream_path_for_session)
            .unwrap_or_default();

        let cc_sid = item.session_ids.worker.as_deref().unwrap_or("");
        let pid = pid_registry::get_pid(cc_sid).unwrap_or(0);
        let process_alive = pid > 0 && mando_cc::is_process_alive(pid);
        let stream_tail = crate::io::transcript::extract_stream_tail(&stream_path, 50);
        let stream_stale_s = mando_cc::stream_stale_seconds(&stream_path);
        let prev_cpu_time_s =
            health_store::get_health_f64(health_state, worker_name.as_str(), "cpu_time_s");

        let seconds_active = seconds_since(item.worker_started_at.as_deref());

        // Build PR discovery args if needed.
        let discover_pr = if item.pr.is_none() {
            item.branch.as_deref().and_then(|branch| {
                let repo = item
                    .project
                    .as_deref()
                    .and_then(|name| mando_config::resolve_project_config(Some(name), config))
                    .and_then(|(_, pc)| pc.github_repo.clone())?;
                Some((repo, branch.to_string()))
            })
        } else {
            None
        };

        // Resolve the GitHub repo slug from config for short PR ref resolution.
        let github_repo = mando_config::resolve_github_repo(item.project.as_deref(), config);

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
            existing_pr: item.pr.clone(),
            item_title: item.title.clone(),
            github_repo,
            branch: item.branch.clone(),
            intervention_count: item.intervention_count,
            no_pr: item.no_pr,
            reopen_seq: item.reopen_seq,
            reopen_source: item.reopen_source.clone(),
        });
    }

    if work.is_empty() {
        return Vec::new();
    }

    // Phase 2: run GitHub API calls in parallel across all workers.
    let futures: Vec<_> = work.iter().map(gather_one_async).collect();
    let results = futures::future::join_all(futures).await;

    // Phase 3: apply discovered PRs back and assemble contexts.
    let mut contexts = Vec::with_capacity(work.len());
    for (w, result) in work.iter().zip(results) {
        // Write discovered PR back to item.
        if let Some(ref pr_url) = result.discovered_pr {
            items[w.item_idx].pr = Some(pr_url.clone());
        }

        let pr = result
            .discovered_pr
            .as_ref()
            .or(w.existing_pr.as_ref())
            .cloned();

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
            github_repo_configured: w.github_repo.is_some(),
        });
    }

    contexts
}

/// Sync-local data collected in phase 1 for each worker.
struct GatherWork {
    item_idx: usize,
    worker_name: String,
    pid: u32,
    process_alive: bool,
    stream_tail: String,
    stream_stale_s: Option<f64>,
    prev_cpu_time_s: Option<f64>,
    seconds_active: f64,
    /// (repo, branch) if PR discovery is needed.
    discover_pr: Option<(String, String)>,
    existing_pr: Option<String>,
    item_title: String,
    github_repo: Option<String>,
    branch: Option<String>,
    intervention_count: i64,
    no_pr: bool,
    reopen_seq: i64,
    reopen_source: Option<String>,
}

/// Async results from phase 2 for each worker.
struct GatherResult {
    discovered_pr: Option<String>,
    pr_data: PrData,
    cpu_time_s: Option<f64>,
}

/// Run the async portion of context gathering for one worker (PR discovery + PR data + CPU time).
async fn gather_one_async(w: &GatherWork) -> GatherResult {
    // CPU time (async syscall).
    let cpu_time_s = if w.process_alive && w.pid > 0 {
        mando_cc::get_cpu_time(w.pid).await.ok()
    } else {
        None
    };

    // PR discovery.
    let discovered_pr = if let Some((ref repo, ref branch)) = w.discover_pr {
        let pr_url = github::discover_pr_for_branch(repo, branch).await;
        if let Some(ref url) = pr_url {
            tracing::info!(
                module = "captain",
                worker = %w.worker_name,
                pr = %url,
                "discovered PR for branch"
            );
        }
        pr_url
    } else {
        None
    };

    // Build a minimal Task to pass to fetch_pr_data.
    let effective_pr = discovered_pr.as_ref().or(w.existing_pr.as_ref()).cloned();
    let stub = Task {
        pr: effective_pr,
        project: w.github_repo.clone(),
        ..Task::new("")
    };
    let pr_data = fetch_pr_data(&stub).await;

    GatherResult {
        discovered_pr,
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
pub(crate) async fn fetch_pr_data(item: &Task) -> PrData {
    let pr_url = match &item.pr {
        Some(pr) => pr,
        None => return PrData::default(),
    };

    // Parse repo and PR number from URL like "https://github.com/owner/repo/pull/123",
    // or resolve short refs like "#12" using the task's project field.
    let parts: Vec<&str> = pr_url.trim_end_matches('/').split('/').collect();
    let (repo, pr_number_str) = if parts.len() >= 5 {
        let repo = format!("{}/{}", parts[parts.len() - 4], parts[parts.len() - 3]);
        let num = parts[parts.len() - 1];
        (repo, num.to_string())
    } else if let Some(num) = mando_types::task::extract_pr_number(pr_url) {
        // Short ref (bare number or "#12") — resolve using task.project.
        if let Some(project) = &item.project {
            (project.clone(), num.to_string())
        } else {
            tracing::warn!(
                module = "captain",
                pr = %pr_url,
                "short PR ref with no project — cannot resolve"
            );
            return PrData::default();
        }
    } else {
        return PrData::default();
    };

    match github::fetch_pr_status(&repo, &pr_number_str).await {
        Ok(status) => {
            let mut degraded = false;

            let ahead = github::is_pr_branch_ahead(&repo, &pr_number_str)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(module = "captain", pr_url = %pr_url, error = %e, "branch-ahead check failed");
                    degraded = true;
                    false
                });

            let pr_num: u32 = pr_number_str.parse().unwrap_or(0);

            // Compute PR hygiene from structured thread/comment data.
            let mut comment_bodies: Vec<String> = Vec::new();
            let (hygiene_unresolved, hygiene_unreplied, unaddressed) = if pr_num > 0 {
                let thread_counts = match github_pr::get_pr_review_threads(&repo, pr_num).await {
                    Ok(threads) => review_marshal::thread_hygiene(&threads, &status.author),
                    Err(e) => {
                        tracing::warn!(module = "captain", pr = pr_num, error = %e, "GraphQL review threads fetch failed, falling back to zeros");
                        degraded = true;
                        (status.unresolved_threads, status.unreplied_threads)
                    }
                };
                let issue_count = if status.comments > 0 {
                    match github_pr::get_pr_comments(&repo, pr_num).await {
                        Ok(comments) => {
                            comment_bodies = comments.iter().map(|c| c.body.clone()).collect();
                            review_marshal::issue_comment_hygiene(&comments, &status.author)
                        }
                        Err(e) => {
                            tracing::warn!(module = "captain", pr = pr_num, error = %e, "issue comments fetch failed, falling back to zero");
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
            tracing::warn!(module = "captain", pr_url = %pr_url, error = %e, "failed to fetch PR status");
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
pub(crate) async fn build_single_context(
    item: &Task,
    config: &mando_config::Config,
) -> (mando_types::WorkerContext, String) {
    use crate::biz::worker_context;
    use crate::io::process_manager;

    let worker_name = item.worker.as_deref().unwrap_or("unknown");
    let stream_path = item
        .session_ids
        .worker
        .as_deref()
        .map(mando_config::stream_path_for_session)
        .unwrap_or_default();
    let stream_tail = crate::io::transcript::extract_stream_tail(&stream_path, 50);
    let stream_stale_s = process_manager::stream_stale_seconds(&stream_path);

    let seconds_active = seconds_since(item.worker_started_at.as_deref());

    // Resolve github repo slug for short PR ref resolution.
    let github_repo = mando_config::resolve_github_repo(item.project.as_deref(), config);
    let has_github_repo = github_repo.is_some();
    let stub = Task {
        pr: item.pr.clone(),
        project: github_repo,
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

    let ctx = WorkerContext {
        session_name: worker_name.to_string(),
        item_title: item.title.clone(),
        status: "in-progress".into(),
        branch: item.branch.clone(),
        pr: item.pr.clone(),
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
        github_repo_configured: has_github_repo,
    };
    let formatted = worker_context::format_context(&ctx);
    (ctx, formatted)
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
