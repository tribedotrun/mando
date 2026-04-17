//! Background auto-titling for terminal workbenches.
//!
//! When a CC session starts in a terminal, the route handler persists the
//! session ID in `workbenches.pending_title_session`. This module runs a
//! periodic loop that reads the first user message from the CC session JSONL,
//! summarizes it via `claude -p`, and updates the workbench title.
//!
//! Fully resumable: intent lives in the DB, so daemon restarts pick up
//! where they left off.

use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::AppState;

/// Periodically process workbenches with `pending_title_session` set.
/// Also wakes immediately when `auto_title_notify` is signalled (e.g. on
/// the first user prompt in a terminal workbench).
pub fn spawn(state: &AppState) {
    let pool = state.db.pool().clone();
    let bus = state.bus.clone();
    let workflow = state.captain_workflow.clone();
    let cancel = state.cancellation_token.clone();
    let notify = state.auto_title_notify.clone();

    state.task_tracker.spawn(async move {
        let initial_interval = workflow.load().auto_title.poll_interval_s;
        tokio::select! {
            _ = tokio::time::sleep(initial_interval) => {}
            _ = notify.notified() => {}
            _ = cancel.cancelled() => { return; }
        }

        loop {
            let cfg = workflow.load().auto_title.clone();
            process_pending(&pool, &bus, &cfg).await;
            tokio::select! {
                _ = tokio::time::sleep(cfg.poll_interval_s) => {}
                _ = notify.notified() => {}
                _ = cancel.cancelled() => {
                    info!(module = "auto-title", "auto-title loop cancelled");
                    return;
                }
            }
        }
    });
}

async fn process_pending(
    pool: &SqlitePool,
    bus: &global_bus::EventBus,
    cfg: &settings::config::workflow::AutoTitleConfig,
) {
    let pending = match captain::io::queries::workbenches::list_pending_title(pool).await {
        Ok(p) => p,
        Err(e) => {
            warn!(module = "auto-title", error = %e, "failed to list pending titles");
            return;
        }
    };

    let home = std::env::var("HOME").unwrap_or_default();
    let projects_dir = std::path::PathBuf::from(&home).join(".claude/projects");

    for row in &pending {
        match title_one(pool, row, &projects_dir, cfg).await {
            Ok(Some(title)) => {
                if let Err(e) =
                    captain::io::queries::workbenches::update_title(pool, row.id, &title).await
                {
                    warn!(module = "auto-title", workbench_id = row.id, error = %e, "update_title failed");
                    continue;
                }
                clear(pool, row.id).await;
                bus.send(global_types::BusEvent::Workbenches, None);
                info!(module = "auto-title", workbench_id = row.id, title = %title, "auto-titled terminal workbench");
            }
            Ok(None) => {
                // Not ready yet. Check expiry based on created_at -- intentionally
                // generous since we don't track when pending_title_session was set.
                if expired(&row.created_at, cfg.expiry_s.as_secs() as i64) {
                    clear(pool, row.id).await;
                    info!(
                        module = "auto-title",
                        workbench_id = row.id,
                        "gave up auto-titling (expired)"
                    );
                }
            }
            Err(e) => {
                warn!(module = "auto-title", workbench_id = row.id, error = %e, "auto-title failed permanently");
                clear(pool, row.id).await;
            }
        }
    }
}

/// Try to auto-title a single workbench. Returns:
/// - `Ok(Some(title))` on success
/// - `Ok(None)` if not ready yet (JSONL or user message missing), or already titled
/// - `Err` on permanent failure
async fn title_one(
    pool: &SqlitePool,
    row: &captain::io::queries::workbenches::PendingTitleRow,
    projects_dir: &std::path::Path,
    cfg: &settings::config::workflow::AutoTitleConfig,
) -> anyhow::Result<Option<String>> {
    // Guard: skip if the workbench title was already changed by something else.
    if let Ok(Some(current)) =
        captain::io::queries::workbenches::find_by_worktree(pool, &row.worktree).await
    {
        if current.title != row.title {
            // Already titled externally -- just clear the pending flag.
            return Ok(None);
        }
    }

    // Find the session JSONL.
    let target = format!("{}.jsonl", row.pending_title_session);
    let sanitized = row.worktree.replace('/', "-");
    let deterministic = projects_dir.join(&sanitized).join(&target);

    let jsonl_path = match find_jsonl(&deterministic, projects_dir, &target).await {
        Some(p) => p,
        None => return Ok(None),
    };

    // Read the first user message.
    let messages = global_claude::transcript::parse_messages(&jsonl_path, Some(5), 0);
    let first_user = match messages.into_iter().find(|m| m.role == "user") {
        Some(m) => m,
        None => return Ok(None),
    };

    let prompt_text: String = first_user.text.chars().take(cfg.max_input_chars).collect();
    if prompt_text.trim().is_empty() {
        return Ok(None);
    }

    // Run claude -p with the configured model and timeout.
    let claude = global_claude::resolve_claude_binary();
    let full_prompt = format!("{}\n\n{prompt_text}", cfg.prompt);
    let cmd = tokio::process::Command::new(&claude)
        .args(["-p", &full_prompt, "--model", &cfg.model])
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(cfg.timeout_s, cmd)
        .await
        .map_err(|_| anyhow::anyhow!("claude -p timed out"))??;

    if !output.status.success() {
        anyhow::bail!(
            "claude -p failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Sanitize LLM output: take first line, strip surrounding quotes.
    let raw = String::from_utf8_lossy(&output.stdout);
    let title = raw
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_string();
    if title.is_empty() {
        anyhow::bail!("haiku returned empty title");
    }
    Ok(Some(title))
}

async fn clear(pool: &SqlitePool, id: i64) {
    if let Err(e) = captain::io::queries::workbenches::clear_pending_title_session(pool, id).await {
        warn!(module = "auto-title", workbench_id = id, error = %e, "clear_pending failed");
    }
}

fn expired(created_at: &str, max_age_secs: i64) -> bool {
    let Ok(created) =
        time::OffsetDateTime::parse(created_at, &time::format_description::well_known::Rfc3339)
    else {
        return true;
    };
    let age = time::OffsetDateTime::now_utc() - created;
    age.whole_seconds() > max_age_secs
}

async fn find_jsonl(
    deterministic: &std::path::Path,
    projects_dir: &std::path::Path,
    target: &str,
) -> Option<std::path::PathBuf> {
    match tokio::fs::try_exists(deterministic).await {
        Ok(true) => return Some(deterministic.to_path_buf()),
        Err(e) => warn!(
            module = "auto-title",
            path = %deterministic.display(),
            error = %e,
            "failed to check session JSONL path"
        ),
        _ => {}
    }
    match tokio::fs::read_dir(projects_dir).await {
        Ok(mut entries) => {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let candidate = entry.path().join(target);
                if tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
                    return Some(candidate);
                }
            }
        }
        Err(e) => warn!(
            module = "auto-title",
            path = %projects_dir.display(),
            error = %e,
            "failed to scan claude projects dir"
        ),
    }
    None
}
