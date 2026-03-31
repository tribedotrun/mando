use clap::Subcommand;
use serde_json::{json, Value};

use crate::http::DaemonClient;

#[derive(Subcommand)]
pub(crate) enum KnowledgeCommand {
    /// List pending lessons awaiting approval
    Pending,
    /// List approved lessons
    Approved,
    /// Approve lessons by ID or approve all pending lessons
    Approve {
        /// Lesson IDs to approve (ignored when --all is set)
        ids: Vec<String>,
        /// Approve every pending lesson
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum PatternsCommand {
    /// Run the pattern distiller
    Run,
    /// Show existing patterns
    List {
        /// Filter patterns by status (pending/approved/dismissed)
        #[arg(long)]
        status: Option<String>,
    },
    /// Approve a pending pattern
    Approve {
        /// Pattern ID
        id: i64,
    },
    /// Dismiss a pending pattern
    Dismiss {
        /// Pattern ID
        id: i64,
    },
}

pub(crate) async fn handle_knowledge(command: Option<KnowledgeCommand>) -> anyhow::Result<()> {
    match command.unwrap_or(KnowledgeCommand::Pending) {
        KnowledgeCommand::Pending => handle_knowledge_pending().await,
        KnowledgeCommand::Approved => handle_knowledge_approved().await,
        KnowledgeCommand::Approve { ids, all } => handle_knowledge_approve(ids, all).await,
    }
}

pub(crate) async fn handle_journal(worker: Option<&str>, limit: usize) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut url = format!("/api/journal?limit={limit}");
    if let Some(w) = worker {
        url.push_str(&format!("&worker={w}"));
    }
    let result = client.get(&url).await?;

    if let Some(totals) = result.get("totals") {
        let total = totals["total"].as_i64().unwrap_or(0);
        let successes = totals["successes"].as_i64().unwrap_or(0);
        let failures = totals["failures"].as_i64().unwrap_or(0);
        let unresolved = totals["unresolved"].as_i64().unwrap_or(0);
        println!(
            "Journal: {total} total | {successes} success | {failures} failure | {unresolved} unresolved"
        );
        println!("{}", "-".repeat(80));
    }

    if let Some(decisions) = result["decisions"].as_array() {
        for d in decisions {
            let worker = d["worker"].as_str().unwrap_or("?");
            let action = d["action"].as_str().unwrap_or("?");
            let outcome = d["outcome"].as_str().unwrap_or("pending");
            let rule = d["rule"].as_str().unwrap_or("?");
            let rule_short: String = rule.chars().take(50).collect();
            let created = d["created_at"].as_str().unwrap_or("?");
            println!("{created}  {worker:<20} {action:<16} {outcome:<10} {rule_short}");
        }
    }
    Ok(())
}

pub(crate) async fn handle_patterns(command: Option<PatternsCommand>) -> anyhow::Result<()> {
    match command.unwrap_or(PatternsCommand::Run) {
        PatternsCommand::Run => handle_patterns_run().await,
        PatternsCommand::List { status } => handle_patterns_list(status.as_deref()).await,
        PatternsCommand::Approve { id } => handle_pattern_update(id, "approved").await,
        PatternsCommand::Dismiss { id } => handle_pattern_update(id, "dismissed").await,
    }
}

async fn handle_knowledge_pending() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get("/api/knowledge/pending").await?;
    print_lessons(&result["pending"], "Pending knowledge lessons")
}

async fn handle_knowledge_approved() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client.get("/api/knowledge").await?;
    print_lessons(&result["approved"], "Approved knowledge lessons")
}

async fn handle_knowledge_approve(ids: Vec<String>, all: bool) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let pending = client.get("/api/knowledge/pending").await?;
    let lessons = pending["pending"].as_array().cloned().unwrap_or_default();

    if lessons.is_empty() {
        println!("No pending lessons to approve.");
        return Ok(());
    }

    let selected = if all {
        lessons
    } else {
        if ids.is_empty() {
            anyhow::bail!("provide lesson IDs or use --all");
        }
        let wanted: std::collections::HashSet<_> = ids.into_iter().collect();
        let filtered: Vec<Value> = lessons
            .into_iter()
            .filter(|lesson| {
                lesson["id"]
                    .as_str()
                    .map(|id| wanted.contains(id))
                    .unwrap_or(false)
            })
            .collect();
        if filtered.is_empty() {
            anyhow::bail!("none of the requested lesson IDs are pending");
        }
        filtered
    };

    let result = client
        .post("/api/knowledge/approve", &json!({"lessons": selected}))
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn print_lessons(value: &Value, label: &str) -> anyhow::Result<()> {
    let lessons = value.as_array().cloned().unwrap_or_default();
    println!("{label}");
    println!("{}", "-".repeat(label.len()));
    if lessons.is_empty() {
        println!("(none)");
        return Ok(());
    }

    for lesson in lessons {
        let id = lesson["id"].as_str().unwrap_or("?");
        let title = lesson["title"].as_str().unwrap_or("Untitled");
        let source = lesson["source"].as_str().unwrap_or("unknown");
        println!("- {id}: {title} ({source})");
    }
    Ok(())
}

async fn handle_patterns_run() -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    println!("Running pattern distiller...");
    let result = client.post("/api/knowledge/learn", &json!({})).await?;
    let summary = result["summary"].as_str().unwrap_or("Done.");
    let found = result["patterns_found"].as_i64().unwrap_or(0);
    println!("{summary}");
    if found > 0 {
        if let Some(patterns) = result["patterns"].as_array() {
            println!();
            for p in patterns {
                let pattern = p["pattern"].as_str().unwrap_or("?");
                let recommendation = p["recommendation"].as_str().unwrap_or("?");
                let confidence = p["confidence"].as_f64().unwrap_or(0.0);
                println!(
                    "  - {pattern} (confidence: {:.0}%)\n    → {recommendation}",
                    confidence * 100.0
                );
            }
        }
    }
    Ok(())
}

async fn handle_patterns_list(status: Option<&str>) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let mut url = "/api/patterns".to_string();
    if let Some(s) = status {
        url.push_str(&format!("?status={s}"));
    }
    let result = client.get(&url).await?;
    if let Some(patterns) = result["patterns"].as_array() {
        if patterns.is_empty() {
            println!("No patterns found.");
        }
        for p in patterns {
            let id = p["id"].as_i64().unwrap_or(0);
            let status = p["status"].as_str().unwrap_or("?");
            let confidence = p["confidence"].as_f64().unwrap_or(0.0);
            let pattern = p["pattern"].as_str().unwrap_or("?");
            let recommendation = p["recommendation"].as_str().unwrap_or("?");
            println!("#{id} [{status}] (confidence: {:.0}%)", confidence * 100.0);
            println!("  Pattern: {pattern}");
            println!("  Recommendation: {recommendation}");
            println!();
        }
    }
    Ok(())
}

async fn handle_pattern_update(id: i64, status: &str) -> anyhow::Result<()> {
    let client = DaemonClient::discover()?;
    let result = client
        .post("/api/patterns/update", &json!({"id": id, "status": status}))
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
