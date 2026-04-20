//! Display-only task subcommands (show, list).

use crate::http::{parse_id, DaemonClient};

fn item_status_label(status: api_types::ItemStatus) -> &'static str {
    match status {
        api_types::ItemStatus::New => "new",
        api_types::ItemStatus::Clarifying => "clarifying",
        api_types::ItemStatus::NeedsClarification => "needs-clarification",
        api_types::ItemStatus::Queued => "queued",
        api_types::ItemStatus::InProgress => "in-progress",
        api_types::ItemStatus::CaptainReviewing => "captain-reviewing",
        api_types::ItemStatus::CaptainMerging => "captain-merging",
        api_types::ItemStatus::AwaitingReview => "awaiting-review",
        api_types::ItemStatus::Rework => "rework",
        api_types::ItemStatus::HandedOff => "handed-off",
        api_types::ItemStatus::Escalated => "escalated",
        api_types::ItemStatus::Errored => "errored",
        api_types::ItemStatus::Merged => "merged",
        api_types::ItemStatus::CompletedNoPr => "completed-no-pr",
        api_types::ItemStatus::PlanReady => "plan-ready",
        api_types::ItemStatus::Canceled => "canceled",
    }
}

/// Fetch all tasks and return the one matching `id_num`. Errors if the item
/// is not found. The daemon has no single-task-by-id GET endpoint, so we
/// always list and filter client-side.
pub(crate) async fn fetch_task_by_id(
    client: &DaemonClient,
    id_num: i64,
) -> anyhow::Result<api_types::TaskItem> {
    let resp: api_types::TaskListResponse =
        client.get_json("/api/tasks?include_archived=true").await?;
    resp.items
        .into_iter()
        .find(|item| item.id == id_num)
        .ok_or_else(|| anyhow::anyhow!("item #{id_num} not found"))
}

pub(crate) async fn handle_show(item_id: &str) -> anyhow::Result<()> {
    let id_num = parse_id(item_id, "item")?;
    let client = DaemonClient::discover()?;
    let item = fetch_task_by_id(&client, id_num).await?;

    let status = item_status_label(item.status);
    let title = item.title.as_str();
    let project = item.project.as_deref().unwrap_or("?");
    let worker = item.worker.as_deref().unwrap_or("-");
    let worktree = item.worktree.as_deref().unwrap_or("-");
    let pr = item
        .pr_number
        .map(|n| format!("#{n}"))
        .unwrap_or_else(|| "-".into());
    let created = item.created_at.as_deref().unwrap_or("?");
    let last_activity = item.last_activity_at.as_deref().unwrap_or("-");
    let worker_seq = item.worker_seq;
    let reopen_seq = item.reopen_seq;
    let intervention = item.intervention_count;

    println!("Task #{item_id}: {title}");
    println!("{}", "-".repeat(60));
    println!("  Status:        {status}");
    println!("  Project:       {project}");
    println!("  Worker:        {worker}");
    println!("  Worktree:      {worktree}");
    println!("  PR:            {pr}");
    println!("  Created:       {created}");
    println!("  Last activity: {last_activity}");
    println!("  Worker seq:    {worker_seq}");
    println!("  Reopen seq:    {reopen_seq}");
    println!("  Interventions: {intervention}");

    // Fetch sessions for this task.
    let sessions_result: api_types::ItemSessionsResponse = client
        .get_json(&format!("/api/tasks/{item_id}/sessions"))
        .await?;
    if !sessions_result.sessions.is_empty() {
        println!("\n  Sessions ({}):", sessions_result.sessions.len());
        for s in sessions_result.sessions {
            let sid = s.session_id;
            let caller = s.caller;
            let cost = s
                .cost_usd
                .map(|c| format!("${c:.2}"))
                .unwrap_or_else(|| "-".into());
            let dur = s
                .duration_ms
                .map(|d| format!("{}s", d / 1000))
                .unwrap_or_else(|| "-".into());
            let status = match s.status {
                api_types::SessionStatus::Running => "running",
                api_types::SessionStatus::Stopped => "stopped",
                api_types::SessionStatus::Failed => "failed",
            };
            println!("    {sid}  {caller:<22}  {dur:>6}  {cost:>8}  {status}");
        }
    }

    Ok(())
}

pub(crate) async fn handle_list(all: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let path = if all {
        "/api/tasks?include_archived=true"
    } else {
        "/api/tasks"
    };
    let resp: api_types::TaskListResponse = client.get_json(path).await?;

    println!(
        "{:>4}  {:<15}  {:<20}  {:<14}  {:<8}  TITLE",
        "ID", "STATUS", "WORKER", "PR", "PROJECT"
    );
    println!("{}", "-".repeat(95));

    for item in resp.items {
        let status = item_status_label(item.status);
        let id = item.id.to_string();
        let project_full = item.project.as_deref().unwrap_or("");
        let project = project_full.rsplit('/').next().unwrap_or(project_full);
        let worker = item.worker.as_deref().unwrap_or("");
        let pr = item.pr_number.map(|n| format!("#{n}")).unwrap_or_default();
        let title = item.title;
        println!("{id:>4}  {status:<15}  {worker:<20}  {pr:<14}  {project:<8}  {title}");
    }
    Ok(())
}
