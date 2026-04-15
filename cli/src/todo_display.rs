//! Display-only task subcommands (show, list).

use crate::http::{parse_id, DaemonClient};

/// Extract an ID from a JSON value that may be a number or a string.
pub(crate) fn id_from_value(v: &serde_json::Value) -> String {
    v.as_i64()
        .map(|n| n.to_string())
        .or_else(|| v.as_str().map(String::from))
        .unwrap_or_else(|| "?".into())
}

/// Fetch all tasks and return the one matching `id_num`. Errors if the item
/// is not found. The daemon has no single-task-by-id GET endpoint, so we
/// always list and filter client-side.
pub(crate) async fn fetch_task_by_id(
    client: &DaemonClient,
    id_num: i64,
) -> anyhow::Result<serde_json::Value> {
    let resp = client.get("/api/tasks?include_archived=true").await?;
    let arr = resp
        .get("items")
        .and_then(|v| v.as_array())
        .or_else(|| resp.as_array());
    arr.and_then(|a| a.iter().find(|it| it["id"].as_i64() == Some(id_num)))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("item #{id_num} not found"))
}

pub(crate) async fn handle_show(item_id: &str) -> anyhow::Result<()> {
    let id_num = parse_id(item_id, "item")?;
    let client = DaemonClient::discover()?;
    let item = fetch_task_by_id(&client, id_num).await?;

    let status = item["status"].as_str().unwrap_or("?");
    let title = item["title"].as_str().unwrap_or("?");
    let project = item["project"].as_str().unwrap_or("?");
    let worker = item["worker"].as_str().unwrap_or("-");
    let worktree = item["worktree"].as_str().unwrap_or("-");
    let pr = item["pr_number"]
        .as_i64()
        .map(|n| format!("#{n}"))
        .unwrap_or_else(|| "-".into());
    let created = item["created_at"].as_str().unwrap_or("?");
    let last_activity = item["last_activity_at"].as_str().unwrap_or("-");
    let worker_seq = item["worker_seq"].as_i64().unwrap_or(0);
    let reopen_seq = item["reopen_seq"].as_i64().unwrap_or(0);
    let intervention = item["intervention_count"].as_i64().unwrap_or(0);

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
    let sessions_result = client
        .get(&format!("/api/tasks/{item_id}/sessions"))
        .await?;
    let empty = vec![];
    let sessions = sessions_result["sessions"].as_array().unwrap_or(&empty);
    if !sessions.is_empty() {
        println!("\n  Sessions ({}):", sessions.len());
        for s in sessions {
            let sid = s["session_id"].as_str().unwrap_or("?");
            let caller = s["caller"].as_str().unwrap_or("?");
            let cost = s["cost_usd"]
                .as_f64()
                .map(|c| format!("${c:.2}"))
                .unwrap_or_else(|| "-".into());
            let dur = s["duration_ms"]
                .as_i64()
                .map(|d| format!("{}s", d / 1000))
                .unwrap_or_else(|| "-".into());
            let status = s["status"].as_str().unwrap_or("?");
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
    let resp = client.get(path).await?;
    let arr = resp
        .get("items")
        .and_then(|v| v.as_array())
        .or_else(|| resp.as_array());

    println!(
        "{:>4}  {:<15}  {:<20}  {:<14}  {:<8}  TITLE",
        "ID", "STATUS", "WORKER", "PR", "PROJECT"
    );
    println!("{}", "-".repeat(95));

    if let Some(items) = arr {
        for item in items {
            let status = item["status"].as_str().unwrap_or("unknown");
            let id = id_from_value(&item["id"]);
            let project_full = item["project"].as_str().unwrap_or("");
            let project = project_full.rsplit('/').next().unwrap_or(project_full);
            let worker = item["worker"].as_str().unwrap_or("");
            let pr = item["pr_number"]
                .as_i64()
                .map(|n| format!("#{n}"))
                .unwrap_or_default();
            let title = item["title"].as_str().unwrap_or("");
            println!("{id:>4}  {status:<15}  {worker:<20}  {pr:<14}  {project:<8}  {title}");
        }
    }
    Ok(())
}
