use sqlx::SqlitePool;
use tracing::{info, warn};

use super::CaptainRuntime;

pub(super) fn spawn(runtime: &CaptainRuntime) {
    let pool = runtime.pool().clone();
    let bus = runtime.bus().clone();
    let settings = runtime.settings().clone();
    let cancel = runtime.cancellation_token().clone();
    let notify = runtime.auto_title_notify().clone();

    runtime.task_tracker().spawn(async move {
        let initial_interval = settings.load_captain_workflow().auto_title.poll_interval_s;
        tokio::select! {
            _ = tokio::time::sleep(initial_interval) => {}
            _ = notify.notified() => {}
            _ = cancel.cancelled() => { return; }
        }

        loop {
            let cfg = settings.load_captain_workflow().auto_title.clone();
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
    cfg: &settings::AutoTitleConfig,
) {
    let pending = match crate::io::queries::workbenches::list_pending_title(pool).await {
        Ok(pending) => pending,
        Err(err) => {
            warn!(module = "auto-title", error = %err, "failed to list pending titles");
            return;
        }
    };

    let home = std::env::var("HOME").unwrap_or_default();
    let projects_dir = std::path::PathBuf::from(&home).join(".claude/projects");

    for row in &pending {
        match title_one(pool, row, &projects_dir, cfg).await {
            Ok(Some(title)) => {
                if let Err(err) =
                    crate::io::queries::workbenches::update_title(pool, row.id, &title).await
                {
                    warn!(module = "auto-title", workbench_id = row.id, error = %err, "update_title failed");
                    continue;
                }
                clear(pool, row.id).await;
                bus.send(global_bus::BusPayload::Workbenches(None));
                info!(module = "auto-title", workbench_id = row.id, title = %title, "auto-titled terminal workbench");
            }
            Ok(None) => {
                if expired(&row.created_at, cfg.expiry_s.as_secs() as i64) {
                    clear(pool, row.id).await;
                    info!(
                        module = "auto-title",
                        workbench_id = row.id,
                        "gave up auto-titling (expired)"
                    );
                }
            }
            Err(err) => {
                warn!(module = "auto-title", workbench_id = row.id, error = %err, "auto-title failed permanently");
                clear(pool, row.id).await;
            }
        }
    }
}

async fn title_one(
    pool: &SqlitePool,
    row: &crate::io::queries::workbenches::PendingTitleRow,
    projects_dir: &std::path::Path,
    cfg: &settings::AutoTitleConfig,
) -> anyhow::Result<Option<String>> {
    if let Ok(Some(current)) =
        crate::io::queries::workbenches::find_by_worktree(pool, &row.worktree).await
    {
        if current.title != row.title {
            return Ok(None);
        }
    }

    let target = format!("{}.jsonl", row.pending_title_session);
    let sanitized = row.worktree.replace('/', "-");
    let deterministic = projects_dir.join(&sanitized).join(&target);

    let jsonl_path = match find_jsonl(&deterministic, projects_dir, &target).await {
        Some(path) => path,
        None => return Ok(None),
    };

    let messages = global_claude::parse_messages(&jsonl_path, Some(5), 0);
    let first_user = match messages.into_iter().find(|message| message.role == "user") {
        Some(message) => message,
        None => return Ok(None),
    };

    let prompt_text: String = first_user.text.chars().take(cfg.max_input_chars).collect();
    if prompt_text.trim().is_empty() {
        return Ok(None);
    }

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
    if let Err(err) = crate::io::queries::workbenches::clear_pending_title_session(pool, id).await {
        warn!(module = "auto-title", workbench_id = id, error = %err, "clear_pending failed");
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
        Err(err) => warn!(
            module = "auto-title",
            path = %deterministic.display(),
            error = %err,
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
        Err(err) => warn!(
            module = "auto-title",
            path = %projects_dir.display(),
            error = %err,
            "failed to scan claude projects dir"
        ),
    }
    None
}
