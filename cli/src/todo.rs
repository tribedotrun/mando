//! `mando todo` — task management CLI (HTTP client).

use clap::{Args, Subcommand};
use serde_json::json;

use crate::http::{parse_id, DaemonClient};

#[derive(Args)]
pub(crate) struct TodoArgs {
    #[command(subcommand)]
    pub command: TodoCommand,
}

#[derive(Subcommand)]
pub(crate) enum TodoCommand {
    /// Add a new task
    Add {
        /// Task title
        title: String,
        /// Project name
        #[arg(short = 'p', long = "project")]
        project: Option<String>,
        /// Plan/brief path for planned handoff
        #[arg(long)]
        plan: Option<String>,
        /// Mark as no-PR / research-only
        #[arg(long)]
        no_pr: bool,
    },
    /// Bulk-add items (one per line or via --stdin)
    Bulk {
        /// Items text (newline-separated)
        items: Option<String>,
        /// Read items from stdin
        #[arg(long)]
        stdin: bool,
        /// Project name
        #[arg(short = 'p', long = "project")]
        project: Option<String>,
    },
    /// Delete a task permanently
    Delete {
        /// Item ID
        item_id: String,
    },
    /// Show details for a single task
    Show {
        /// Item ID
        item_id: String,
    },
    /// List tasks
    List {
        /// Include finalized items
        #[arg(long)]
        all: bool,
    },
    /// Save a work summary for a task
    #[command(name = "summary")]
    Summary {
        /// Item ID (or reads MANDO_TASK_ID from env)
        item_id: Option<String>,
        /// Read summary content from a file
        #[arg(long)]
        file: Option<String>,
    },
    /// Save evidence files for a task
    #[command(name = "evidence")]
    Evidence {
        /// Local file paths to save as evidence
        files: Vec<String>,
        /// Per-file caption (one per file, same order)
        #[arg(long = "caption", num_args = 1)]
        captions: Vec<String>,
    },
    /// Multi-turn Q&A on an item
    Ask {
        /// Item ID
        item_id: String,
        /// Question / message
        message: Option<String>,
        /// End the Q&A session
        #[arg(long)]
        end: bool,
    },
    /// Show lifecycle timeline for an item
    Timeline {
        /// Item ID
        item_id: String,
        /// Show only last N events
        #[arg(long)]
        last: Option<usize>,
    },
    /// Send input/feedback to a task (clarifier or reopen)
    Input {
        /// Item ID
        item_id: String,
        /// Message to send
        message: String,
    },
    /// Show Q&A exchange history for an item
    History {
        /// Item ID
        item_id: String,
    },
}

pub(crate) async fn handle(args: TodoArgs) -> anyhow::Result<()> {
    match args.command {
        TodoCommand::Add {
            title,
            project,
            plan,
            no_pr,
        } => handle_add(&title, project.as_deref(), plan.as_deref(), no_pr).await,
        TodoCommand::Bulk {
            items,
            stdin,
            project,
        } => handle_bulk(items.as_deref(), stdin, project.as_deref()).await,
        TodoCommand::Delete { item_id } => handle_delete(&item_id).await,
        TodoCommand::Show { item_id } => handle_show(&item_id).await,
        TodoCommand::List { all } => handle_list(all).await,
        TodoCommand::Summary { item_id, file } => {
            crate::todo_artifacts::handle_summary(item_id.as_deref(), file.as_deref()).await
        }
        TodoCommand::Evidence { files, captions } => {
            crate::todo_artifacts::handle_evidence(&files, &captions).await
        }
        TodoCommand::Ask {
            item_id,
            message,
            end,
        } => handle_ask(&item_id, message.as_deref(), end).await,
        TodoCommand::Timeline { item_id, last } => handle_timeline(&item_id, last).await,
        TodoCommand::Input { item_id, message } => handle_input(&item_id, &message).await,
        TodoCommand::History { item_id } => handle_history(&item_id).await,
    }
}

async fn handle_add(
    title: &str,
    project: Option<&str>,
    plan: Option<&str>,
    no_pr: bool,
) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut form = reqwest::multipart::Form::new()
        .text("title", title.to_string())
        .text("source", "cli");
    if let Some(p) = project {
        form = form.text("project", p.to_string());
    }
    if let Some(plan_path) = plan {
        form = form.text("plan", plan_path.to_string());
    }
    if no_pr {
        form = form.text("no_pr", "true");
    }
    let result = client.post_multipart("/api/tasks/add", form).await?;
    let id = id_from_value(&result["id"]);
    println!("Added item #{id}: {title}");
    Ok(())
}

async fn handle_bulk(
    items_text: Option<&str>,
    read_stdin: bool,
    project: Option<&str>,
) -> anyhow::Result<()> {
    let text = if read_stdin {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        buf
    } else {
        items_text.unwrap_or("").to_string()
    };

    let client = DaemonClient::discover()?;
    let mut count = 0u32;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut form = reqwest::multipart::Form::new()
            .text("title", line.to_string())
            .text("source", "cli");
        if let Some(p) = project {
            form = form.text("project", p.to_string());
        }
        let result = client.post_multipart("/api/tasks/add", form).await?;
        let id = id_from_value(&result["id"]);
        println!("  #{id}: {line}");
        count += 1;
    }
    println!("Added {count} item(s).");
    Ok(())
}

async fn handle_delete(item_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post(
            "/api/tasks/delete",
            &json!({"ids": [parse_id(item_id, "item")?]}),
        )
        .await?;
    println!("Deleted item #{item_id}.");
    Ok(())
}

async fn handle_show(item_id: &str) -> anyhow::Result<()> {
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

async fn handle_list(all: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let path = if all {
        "/api/tasks?include_archived=true"
    } else {
        "/api/tasks"
    };
    let resp = client.get(path).await?;
    // API returns {"count": N, "items": [...]} or a bare array
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

/// Fetch all tasks and return the one matching `id_num`. Errors if the item
/// is not found. The daemon has no single-task-by-id GET endpoint, so we
/// always list and filter client-side.
async fn fetch_task_by_id(client: &DaemonClient, id_num: i64) -> anyhow::Result<serde_json::Value> {
    let resp = client.get("/api/tasks?include_archived=true").await?;
    let arr = resp
        .get("items")
        .and_then(|v| v.as_array())
        .or_else(|| resp.as_array());
    arr.and_then(|a| a.iter().find(|it| it["id"].as_i64() == Some(id_num)))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("item #{id_num} not found"))
}

async fn handle_ask(item_id: &str, message: Option<&str>, end: bool) -> anyhow::Result<()> {
    let id = parse_id(item_id, "item")?;
    if end {
        let client = DaemonClient::discover()?;
        client
            .post("/api/tasks/ask/end", &json!({"id": id}))
            .await?;
        println!("Ended Q&A session for item #{item_id}.");
        return Ok(());
    }
    match message {
        Some(msg) => {
            let client = DaemonClient::discover()?;
            let body = json!({"id": id, "question": msg});
            let result = client.post("/api/tasks/ask", &body).await?;
            let reply = result["answer"].as_str().unwrap_or("(no reply)");
            println!("{reply}");
        }
        None => {
            println!("Usage: mando todo ask {item_id} \"your question\"");
        }
    }
    Ok(())
}

async fn handle_timeline(item_id: &str, last: Option<usize>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut path = format!("/api/tasks/{item_id}/timeline");
    if let Some(n) = last {
        path = format!("{path}?last={n}");
    }
    let events = client.get(&path).await?;

    let empty = vec![];
    let arr = events.as_array().unwrap_or(&empty);
    if arr.is_empty() {
        println!("No timeline events for item #{item_id}.");
        return Ok(());
    }
    for ev in arr {
        let ts = ev["timestamp"].as_str().unwrap_or("");
        let kind = ev["event_type"].as_str().unwrap_or("");
        let detail = ev["detail"].as_str().unwrap_or("");
        println!("  {ts}  {kind:<20}  {detail}");
    }
    Ok(())
}

async fn handle_input(item_id: &str, message: &str) -> anyhow::Result<()> {
    let id_num = parse_id(item_id, "item")?;
    let client = DaemonClient::discover()?;

    // Fetch current item status.
    let item = fetch_task_by_id(&client, id_num).await?;
    let status = item["status"].as_str().unwrap_or("");

    match status {
        "in-progress" => {
            anyhow::bail!("item #{item_id} has an active worker — use `captain nudge` instead");
        }
        "merged" | "completed-no-pr" | "canceled" | "escalated" | "awaiting-review"
        | "handed-off" | "errored" => {
            // Reopen with feedback.
            let body = json!({"id": id_num, "feedback": message});
            client.post("/api/tasks/reopen", &body).await?;
            println!("Reopened item #{item_id} with feedback.");
        }
        "needs-clarification" => {
            // Use unified clarify endpoint.
            let result = client
                .post(
                    &format!("/api/tasks/{item_id}/clarify"),
                    &json!({"answer": message}),
                )
                .await?;
            let status = result["status"].as_str().unwrap_or("");
            match status {
                "ready" => println!("Clarified — item queued for work."),
                "clarifying" => {
                    let questions = result["questions"].as_str().unwrap_or("");
                    println!("Needs more info: {questions}");
                }
                "escalate" => println!("Escalated to captain review."),
                "needs-clarification" => {
                    let error = result["error"].as_str().unwrap_or("unknown");
                    println!(
                        "Answer saved, but re-clarification failed ({error}). Captain will retry."
                    );
                }
                _ => println!("Answer saved."),
            }
        }
        _ => {
            // Append context via patch.
            let body = json!({"context": format!("[Human input] {message}")});
            client
                .patch(&format!("/api/tasks/{item_id}"), &body)
                .await?;
            println!("Appended input to item #{item_id}.");
        }
    }
    Ok(())
}

async fn handle_history(item_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let entries = client.get(&format!("/api/tasks/{item_id}/history")).await?;

    let empty = vec![];
    let arr = entries["history"].as_array().unwrap_or(&empty);
    if arr.is_empty() {
        println!("No Q&A history for item #{item_id}.");
        return Ok(());
    }

    for entry in arr {
        let ts = entry["timestamp"].as_str().unwrap_or("");
        let role = entry["role"].as_str().unwrap_or("");
        let content = entry["content"].as_str().unwrap_or("");
        println!("  [{ts}] {role}: {content}");
    }
    Ok(())
}

/// Extract an ID from a JSON value that may be a number or a string.
fn id_from_value(v: &serde_json::Value) -> String {
    v.as_i64()
        .map(|n| n.to_string())
        .or_else(|| v.as_str().map(String::from))
        .unwrap_or_else(|| "?".into())
}

#[cfg(test)]
fn is_terminal(status: &str) -> bool {
    matches!(status, "merged" | "completed-no-pr" | "canceled")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCmd,
    }

    #[derive(clap::Subcommand)]
    enum TestCmd {
        Todo(TodoArgs),
    }

    #[test]
    fn parse_todo_add() {
        let cli = TestCli::try_parse_from(["test", "todo", "add", "Fix bug"]).unwrap();
        match cli.cmd {
            TestCmd::Todo(args) => match args.command {
                TodoCommand::Add {
                    title,
                    project,
                    plan,
                    no_pr,
                } => {
                    assert_eq!(title, "Fix bug");
                    assert!(project.is_none());
                    assert!(plan.is_none());
                    assert!(!no_pr);
                }
                _ => panic!("expected Add"),
            },
        }
    }

    #[test]
    fn parse_todo_add_with_project() {
        let cli =
            TestCli::try_parse_from(["test", "todo", "add", "Fix bug", "-p", "mando"]).unwrap();
        match cli.cmd {
            TestCmd::Todo(args) => match args.command {
                TodoCommand::Add { project, .. } => {
                    assert_eq!(project.as_deref(), Some("mando"));
                }
                _ => panic!("expected Add"),
            },
        }
    }

    #[test]
    fn parse_todo_add_with_plan_handoff_flags() {
        let cli = TestCli::try_parse_from([
            "test",
            "todo",
            "add",
            "Ship planned task",
            "--plan",
            "~/.mando/plans/42/brief.md",
            "--no-pr",
        ])
        .unwrap();
        match cli.cmd {
            TestCmd::Todo(args) => match args.command {
                TodoCommand::Add { plan, no_pr, .. } => {
                    assert_eq!(plan.as_deref(), Some("~/.mando/plans/42/brief.md"));
                    assert!(no_pr);
                }
                _ => panic!("expected Add"),
            },
        }
    }

    #[test]
    fn parse_todo_add_rejects_context_flag() {
        // `--context` was removed from the CLI; clap must reject it.
        let result = TestCli::try_parse_from([
            "test",
            "todo",
            "add",
            "Fix bug",
            "--context",
            "some context",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_todo_list_all() {
        let cli = TestCli::try_parse_from(["test", "todo", "list", "--all"]).unwrap();
        match cli.cmd {
            TestCmd::Todo(args) => match args.command {
                TodoCommand::List { all } => assert!(all),
                _ => panic!("expected List"),
            },
        }
    }

    #[test]
    fn parse_todo_delete() {
        let cli = TestCli::try_parse_from(["test", "todo", "delete", "42"]).unwrap();
        match cli.cmd {
            TestCmd::Todo(args) => match args.command {
                TodoCommand::Delete { item_id } => {
                    assert_eq!(item_id, "42")
                }
                _ => panic!("expected Delete"),
            },
        }
    }

    #[test]
    fn parse_todo_show() {
        let cli = TestCli::try_parse_from(["test", "todo", "show", "14"]).unwrap();
        match cli.cmd {
            TestCmd::Todo(args) => match args.command {
                TodoCommand::Show { item_id } => {
                    assert_eq!(item_id, "14");
                }
                _ => panic!("expected Show"),
            },
        }
    }

    #[test]
    fn parse_todo_timeline() {
        let cli =
            TestCli::try_parse_from(["test", "todo", "timeline", "5", "--last", "3"]).unwrap();
        match cli.cmd {
            TestCmd::Todo(args) => match args.command {
                TodoCommand::Timeline { item_id, last, .. } => {
                    assert_eq!(item_id, "5");
                    assert_eq!(last, Some(3));
                }
                _ => panic!("expected Timeline"),
            },
        }
    }

    #[test]
    fn is_terminal_check() {
        assert!(is_terminal("merged"));
        assert!(is_terminal("canceled"));
        assert!(is_terminal("completed-no-pr"));
        assert!(!is_terminal("in-progress"));
        assert!(!is_terminal("new"));
    }
}
