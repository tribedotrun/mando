//! `mando todo` — task management CLI (HTTP client).

use clap::{Args, Subcommand};
use serde_json::json;

use crate::gateway_paths as paths;
use crate::http::{parse_id, DaemonClient};
use crate::todo_display::fetch_task_by_id;

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
        /// Disable auto-merge for this task even if global auto-merge is on
        #[arg(long)]
        no_auto_merge: bool,
        /// Planning/discussion mode -- autonomous plan refinement, no implementation
        #[arg(long)]
        discuss: bool,
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
        /// Per-file role tag, one per file in the same order (`before`,
        /// `after`, `cannot-reproduce`, or `other`). Required for bug-fix
        /// tasks: captain pairs at least one `before` with one `after`
        /// before shipping. Pass `cannot-reproduce` (with a text file
        /// explaining the repro attempt) when the bug cannot be triggered
        /// — captain escalates to the human deterministically. Omit (or
        /// pass `other`) for non-bug-fix tasks.
        #[arg(long = "kind", num_args = 1)]
        kinds: Vec<String>,
        /// Skip the motion check on video files. Use only when "nothing happens" is itself the evidence.
        #[arg(long = "allow-static")]
        allow_static: bool,
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
            no_auto_merge,
            discuss,
        } => {
            handle_add(
                &title,
                project.as_deref(),
                plan.as_deref(),
                no_pr,
                no_auto_merge,
                discuss,
            )
            .await
        }
        TodoCommand::Bulk {
            items,
            stdin,
            project,
        } => handle_bulk(items.as_deref(), stdin, project.as_deref()).await,
        TodoCommand::Delete { item_id } => handle_delete(&item_id).await,
        TodoCommand::Show { item_id } => crate::todo_display::handle_show(&item_id).await,
        TodoCommand::List { all } => crate::todo_display::handle_list(all).await,
        TodoCommand::Summary { item_id, file } => {
            crate::todo_artifacts::handle_summary(item_id.as_deref(), file.as_deref()).await
        }
        TodoCommand::Evidence {
            files,
            captions,
            kinds,
            allow_static,
        } => crate::todo_artifacts::handle_evidence(&files, &captions, &kinds, allow_static).await,
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
    no_auto_merge: bool,
    discuss: bool,
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
    if no_auto_merge {
        form = form.text("no_auto_merge", "true");
    }
    if discuss {
        form = form.text("planning", "true");
    }
    let result: api_types::TaskItem = client.post_multipart_json(paths::TASKS_ADD, form).await?;
    println!("Added item #{}: {title}", result.id);
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
        let result: api_types::TaskItem =
            client.post_multipart_json(paths::TASKS_ADD, form).await?;
        println!("  #{}: {line}", result.id);
        count += 1;
    }
    println!("Added {count} item(s).");
    Ok(())
}

async fn handle_delete(item_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    client
        .post_json::<api_types::DeleteTasksResponse, _>(
            paths::TASKS_DELETE,
            &json!({"ids": [parse_id(item_id, "item")?]}),
        )
        .await?;
    println!("Deleted item #{item_id}.");
    Ok(())
}

async fn handle_ask(item_id: &str, message: Option<&str>, end: bool) -> anyhow::Result<()> {
    let id = parse_id(item_id, "item")?;
    if end {
        let client = DaemonClient::discover()?;
        client
            .post_json::<api_types::AskEndResponse, _>(
                paths::TASKS_ASK_END,
                &api_types::TaskIdRequest { id },
            )
            .await?;
        println!("Ended Q&A session for item #{item_id}.");
        return Ok(());
    }
    match message {
        Some(msg) => {
            let client = DaemonClient::discover()?;
            let result: api_types::AskResponse = client
                .post_json(
                    paths::TASKS_ASK,
                    &api_types::TaskAskRequest {
                        id,
                        question: msg.to_string(),
                        ask_id: None,
                    },
                )
                .await?;
            println!(
                "{}",
                if result.answer.is_empty() {
                    "(no reply)"
                } else {
                    result.answer.as_str()
                }
            );
        }
        None => {
            println!("Usage: mando todo ask {item_id} \"your question\"");
        }
    }
    Ok(())
}

async fn handle_timeline(item_id: &str, last: Option<usize>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let path = match last {
        Some(n) => paths::task_timeline_last(item_id, n),
        None => paths::task_timeline(item_id),
    };
    let events: api_types::TimelineResponse = client.get_json(&path).await?;

    if events.events.is_empty() {
        println!("No timeline events for item #{item_id}.");
        return Ok(());
    }
    for ev in &events.events {
        println!(
            "  {}  {:<20}  {}",
            ev.timestamp,
            ev.data.event_type_str(),
            ev.summary
        );
    }
    Ok(())
}

async fn handle_input(item_id: &str, message: &str) -> anyhow::Result<()> {
    let id_num = parse_id(item_id, "item")?;
    let client = DaemonClient::discover()?;

    // Fetch current item status.
    let item = fetch_task_by_id(&client, id_num).await?;
    match item.status {
        api_types::ItemStatus::InProgress => {
            anyhow::bail!("item #{item_id} has an active worker — use `captain nudge` instead");
        }
        api_types::ItemStatus::PlanReady => {
            // Re-queue as a normal worker with the plan injected into context.
            let timeline: api_types::TimelineResponse =
                client.get_json(&paths::task_timeline(id_num)).await?;
            let plan_text = timeline
                .events
                .iter()
                .rev()
                .find_map(|e| match &e.data {
                    api_types::TimelineEventPayload::PlanCompleted { plan, .. } => {
                        Some(plan.as_str())
                    }
                    _ => None,
                })
                .unwrap_or("");
            let existing_ctx = item.context.as_deref().unwrap_or("");
            let new_ctx = if plan_text.is_empty() {
                format!("{existing_ctx}\n\n[Human] {message}")
            } else {
                format!("{existing_ctx}\n\n## Approved Plan\n{plan_text}\n\n[Human] {message}")
            };
            client
                .patch_json::<api_types::TaskItem, _>(
                    &paths::task_item(id_num),
                    &api_types::TaskPatchRequest {
                        context: Some(new_ctx),
                        original_prompt: None,
                        is_bug_fix: None,
                    },
                )
                .await?;
            client
                .post_json::<api_types::BoolOkResponse, _>(
                    paths::TASKS_QUEUE,
                    &api_types::TaskIdRequest { id: id_num },
                )
                .await?;
            println!("Re-queued item #{item_id} for implementation with plan.");
        }
        api_types::ItemStatus::Merged
        | api_types::ItemStatus::CompletedNoPr
        | api_types::ItemStatus::Canceled
        | api_types::ItemStatus::Escalated
        | api_types::ItemStatus::AwaitingReview
        | api_types::ItemStatus::HandedOff
        | api_types::ItemStatus::Errored
        | api_types::ItemStatus::Stopped => {
            // Reopen with feedback.
            client
                .post_json::<api_types::BoolOkResponse, _>(
                    paths::TASKS_REOPEN,
                    &api_types::TaskFeedbackRequest {
                        id: id_num,
                        feedback: message.to_string(),
                    },
                )
                .await?;
            println!("Reopened item #{item_id} with feedback.");
        }
        api_types::ItemStatus::NeedsClarification => {
            // Use unified clarify endpoint.
            let result: api_types::ClarifyResponse = client
                .post_json(
                    &paths::task_clarify(item_id),
                    &api_types::ClarifyRequest {
                        answers: None,
                        answer: Some(message.to_string()),
                    },
                )
                .await?;
            match result.status.as_str() {
                "ready" => println!("Clarified — item queued for work."),
                "clarifying" => {
                    let questions = result
                        .questions
                        .unwrap_or_default()
                        .into_iter()
                        .map(|q| q.question)
                        .collect::<Vec<_>>()
                        .join("\n");
                    println!("Needs more info: {questions}");
                }
                "escalate" => println!("Escalated to captain review."),
                "needs-clarification" => {
                    let error = result.error.as_deref().unwrap_or("unknown");
                    println!(
                        "Answer saved, but re-clarification failed ({error}). Captain will retry."
                    );
                }
                _ => println!("Answer saved."),
            }
        }
        _ => {
            // Append context via patch.
            client
                .patch_json::<api_types::TaskItem, _>(
                    &paths::task_item(item_id),
                    &api_types::TaskPatchRequest {
                        context: Some(format!("[Human input] {message}")),
                        original_prompt: None,
                        is_bug_fix: None,
                    },
                )
                .await?;
            println!("Appended input to item #{item_id}.");
        }
    }
    Ok(())
}

async fn handle_history(item_id: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let entries: api_types::AskHistoryResponse =
        client.get_json(&paths::task_history(item_id)).await?;

    if entries.history.is_empty() {
        println!("No Q&A history for item #{item_id}.");
        return Ok(());
    }

    for entry in &entries.history {
        println!("  [{}] {}: {}", entry.timestamp, entry.role, entry.content);
    }
    Ok(())
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
                    no_auto_merge,
                    discuss,
                } => {
                    assert_eq!(title, "Fix bug");
                    assert!(project.is_none());
                    assert!(plan.is_none());
                    assert!(!no_pr);
                    assert!(!no_auto_merge);
                    assert!(!discuss);
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
