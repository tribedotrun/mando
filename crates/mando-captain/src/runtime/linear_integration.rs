//! Linear integration — issue creation, status writeback, workpad lifecycle, sync.
//!
//! Higher-level orchestration on top of `io/linear.rs` CLI wrappers.

use anyhow::Result;
use mando_config::settings::Config;
use mando_types::task::{ItemStatus, Task};

/// Create a Linear issue for a new task.
///
/// Sets the item's `linear_id` field with the created issue identifier.
pub async fn create_issue_for_task(item: &mut Task, config: &Config) -> Result<()> {
    let captain = &config.captain;
    if captain.linear_team.is_empty() {
        return Ok(()); // Linear not configured.
    }

    let cli = crate::io::linear::resolve_cli_path(&captain.linear_cli_path)?;
    let description = item.context.as_deref();

    let labels: Vec<String> = item
        .project
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect();

    match crate::io::linear::create_issue(
        &cli,
        &captain.linear_team,
        &item.title,
        description,
        &labels,
    )
    .await
    {
        Ok(output) => {
            // CLI output: "ENG-42: title\nhttps://...". Extract just the identifier.
            let issue_id = parse_issue_id(&output);
            if !issue_id.is_empty() {
                item.linear_id = Some(issue_id.clone());
                tracing::info!(module = "linear", issue_id = %issue_id, title = %item.title, "created issue");
            } else {
                tracing::warn!(module = "linear", title = %item.title, raw_output = %output, "created issue but failed to parse ID — issue may be orphaned");
            }
        }
        Err(e) => {
            tracing::warn!(module = "linear", title = %item.title, error = %e, "failed to create issue");
        }
    }
    Ok(())
}

/// Writeback task status to Linear issue.
pub(crate) async fn writeback_status(item: &Task, config: &Config) -> Result<()> {
    let linear_id = match &item.linear_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return Ok(()),
    };

    let captain = &config.captain;
    let cli = crate::io::linear::resolve_cli_path(&captain.linear_cli_path)?;

    let linear_status = match item.status {
        ItemStatus::New | ItemStatus::Queued | ItemStatus::NeedsClarification => "Todo",
        ItemStatus::InProgress
        | ItemStatus::Rework
        | ItemStatus::CaptainReviewing
        | ItemStatus::CaptainMerging => "In Progress",
        ItemStatus::AwaitingReview | ItemStatus::HandedOff => "In Review",
        ItemStatus::Merged | ItemStatus::CompletedNoPr => "Done",
        ItemStatus::Canceled => "Canceled",
        ItemStatus::Escalated | ItemStatus::Errored => "Todo",
        ItemStatus::Clarifying => "Backlog",
    };

    if let Err(e) = crate::io::linear::update_status(&cli, &linear_id, linear_status).await {
        tracing::warn!(module = "linear", linear_id = %linear_id, error = %e, "writeback failed");
    } else {
        tracing::debug!(module = "linear", linear_id = %linear_id, status = %linear_status, "writeback complete");
    }
    Ok(())
}

/// Upsert workpad comment on a Linear issue.
///
/// Creates a comment on first call, updates the same comment on subsequent calls.
/// Comment IDs are tracked in the SQLite DB (`linear_workpad` table).
pub(crate) async fn upsert_workpad(
    item: &Task,
    config: &Config,
    body: &str,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let linear_id = match &item.linear_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => return Ok(()),
    };

    let captain = &config.captain;
    let cli = crate::io::linear::resolve_cli_path(&captain.linear_cli_path)?;

    let existing_comment = mando_db::queries::linear_workpad::get(pool, &linear_id)
        .await
        .unwrap_or(None);

    if let Some(ref comment_id) = existing_comment {
        // Try to update existing comment. If it fails (deleted?), fall through to create.
        if crate::io::linear::update_comment(&cli, comment_id, body)
            .await
            .is_ok()
        {
            return Ok(());
        }
        tracing::warn!(module = "linear", linear_id = %linear_id, "workpad update failed, creating new comment");
    }

    // Create new comment and persist its ID to DB immediately.
    // If we crash between API call and DB write, the next call will detect the
    // orphaned comment via the update-first path above (existing_comment check).
    let comment_id = match crate::io::linear::post_comment(&cli, &linear_id, body).await {
        Ok(id) if !id.is_empty() => id,
        Ok(_) => return Ok(()), // Empty ID — CLI didn't return one.
        Err(e) => {
            tracing::warn!(module = "linear", error = %e, "workpad comment failed");
            return Ok(());
        }
    };

    // Persist mapping before returning success — if we crash after this point,
    // the comment ID is already recorded and the next upsert will update it.
    if let Err(e) = mando_db::queries::linear_workpad::upsert(pool, &linear_id, &comment_id).await {
        tracing::error!(
            module = "linear",
            linear_id = %linear_id,
            comment_id = %comment_id,
            error = %e,
            "failed to persist workpad mapping — comment exists on Linear but is untracked"
        );
    }

    Ok(())
}

/// Import Linear "Todo" issues into tasks (tick pre-phase sync).
pub(crate) async fn sync_linear_to_tasks(config: &Config) -> Result<Vec<Task>> {
    let captain = &config.captain;
    if captain.linear_team.is_empty() {
        return Ok(Vec::new());
    }

    let cli = crate::io::linear::resolve_cli_path(&captain.linear_cli_path)?;

    // Search for issues in "Todo" status.
    let results = match crate::io::linear::search_issues(&cli, "status:Todo").await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(module = "linear", error = %e, "sync search failed");
            return Ok(Vec::new());
        }
    };

    // Parse results into task items.
    let mut new_items = Vec::new();
    for line in &results {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() < 2 {
            continue;
        }
        let issue_id = parts[0].trim();
        let title = parts[1].trim();

        // Skip if already in task list (checked by caller).
        let mut item = Task::new(title);
        item.linear_id = Some(issue_id.to_string());
        item.status = ItemStatus::New;
        new_items.push(item);
    }

    tracing::info!(
        module = "linear",
        count = new_items.len(),
        "sync found Todo issues"
    );
    Ok(new_items)
}

/// Filter out items that already exist in the task list.
pub(crate) fn filter_existing(new_items: Vec<Task>, existing: &[Task]) -> Vec<Task> {
    let existing_ids: std::collections::HashSet<&str> = existing
        .iter()
        .filter_map(|it| it.linear_id.as_deref())
        .collect();

    new_items
        .into_iter()
        .filter(|it| {
            it.linear_id
                .as_deref()
                .map(|id| !existing_ids.contains(id))
                .unwrap_or(true)
        })
        .collect()
}

/// Extract issue identifier (e.g. "ENG-42") from Linear CLI create output
/// or a corrupted `linear_id` field.
///
/// The CLI prints "ENG-42: title text\nhttps://linear.app/...".
/// We want only the identifier before the first colon.
pub(crate) fn parse_issue_id(output: &str) -> String {
    // Skip warning/info lines (e.g. "Warning: Label 'x' not found, skipping")
    // and find the first line that looks like an issue identifier.
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = trimmed
            .split_once(':')
            .map(|(id, _)| id.trim())
            .unwrap_or(trimmed);
        // Must look like an issue ID: PREFIX-123.
        if candidate.contains('-')
            && candidate
                .split_once('-')
                .is_some_and(|(_, n)| n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty())
        {
            return candidate.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_issue_id_extracts_identifier() {
        let output = "ENG-968: in scout summary, if it's youtube link\nhttps://linear.app/acme/issue/ENG-968/foo";
        assert_eq!(parse_issue_id(output), "ENG-968");
    }

    #[test]
    fn parse_issue_id_bare_identifier() {
        assert_eq!(parse_issue_id("ENG-42\n"), "ENG-42");
    }

    #[test]
    fn parse_issue_id_rejects_garbage() {
        assert_eq!(parse_issue_id("Warning: label not found"), "");
        assert_eq!(parse_issue_id("Error: something went wrong"), "");
        assert_eq!(parse_issue_id(""), "");
        assert_eq!(parse_issue_id("no-digits-here"), "");
    }

    #[test]
    fn parse_issue_id_skips_warning_lines() {
        let output = "Warning: Label 'sandbox' not found, skipping\nTST-894: debug-probe\nhttps://linear.app/example/issue/TST-894/debug-probe";
        assert_eq!(parse_issue_id(output), "TST-894");
    }

    #[test]
    fn filter_removes_duplicates() {
        let mut existing_item = Task::new("Existing");
        existing_item.linear_id = Some("ENG-1".into());

        let mut new_dup = Task::new("Dup");
        new_dup.linear_id = Some("ENG-1".into());

        let mut new_fresh = Task::new("Fresh");
        new_fresh.linear_id = Some("ENG-2".into());

        let filtered = filter_existing(vec![new_dup, new_fresh], &[existing_item]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].linear_id.as_deref(), Some("ENG-2"));
    }
}
