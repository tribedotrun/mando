use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use settings::CaptainWorkflow;

use crate::io::session_terminate;
use crate::service::lifecycle;
use crate::{ItemStatus, ReviewTrigger, Task, TimelineEventPayload};

use super::notify::Notifier;

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(item, cwd, workflow, notifier, pool, worker_contexts_text), fields(task_id = item.id, trigger, provider = "codex"))]
pub(super) async fn spawn_codex_review(
    item: &mut Task,
    trigger: &str,
    db_status: Option<&str>,
    cwd: std::path::PathBuf,
    parsed_trigger: ReviewTrigger,
    worker_contexts_text: String,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let prompt =
        build_codex_review_prompt(item, trigger, worker_contexts_text, workflow, pool).await?;
    let mut transition_check = item.clone();
    lifecycle::apply_transition(&mut transition_check, ItemStatus::CaptainReviewing)?;

    let started = super::agent_runtime::spawn_structured_session(
        item.provider,
        pool,
        "captain-review-async",
        item.id,
        &item.project,
        "",
        &cwd,
        &prompt,
        super::captain_review::codex_verdict_output_schema(trigger),
        None,
        &workflow.agent,
    )
    .await?;

    let guard_status = db_status
        .map(str::to_string)
        .unwrap_or_else(|| item.status.as_str().to_string());
    let prev_status = item.status;
    let saved_last_activity = item.last_activity_at.clone();
    if let Err(e) = lifecycle::apply_transition(item, ItemStatus::CaptainReviewing) {
        session_terminate::terminate_session(
            pool,
            &started.session_id,
            global_types::SessionStatus::Stopped,
            None,
        )
        .await;
        return Err(e).context("review transition failed after Codex spawn");
    }
    item.captain_review_trigger = Some(parsed_trigger);
    item.last_activity_at = Some(global_types::now_rfc3339());
    item.session_ids.review = Some(started.session_id.clone());

    let event = crate::TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!("Captain review started (trigger: {trigger})"),
        data: TimelineEventPayload::CaptainReviewStarted {
            trigger: trigger.to_string(),
            session_id: started.session_id.clone(),
        },
    };
    match crate::io::queries::tasks::persist_status_transition(pool, item, &guard_status, &event)
        .await
    {
        Ok(true) => {
            notifier
                .normal(&format!(
                    "🔍 Captain reviewing <b>{}</b> (trigger: {trigger})",
                    global_infra::html::escape_html(&item.title),
                ))
                .await;
            tracing::info!(module = "captain", session_id = %started.session_id, "codex review session spawned");
            Ok(())
        }
        Ok(false) => {
            rollback_review_spawn(item, prev_status, saved_last_activity);
            session_terminate::terminate_session(
                pool,
                &started.session_id,
                global_types::SessionStatus::Stopped,
                None,
            )
            .await;
            tracing::info!(
                module = "captain",
                item_id = item.id,
                "review spawn transition already applied"
            );
            Ok(())
        }
        Err(e) => {
            rollback_review_spawn(item, prev_status, saved_last_activity);
            session_terminate::terminate_session(
                pool,
                &started.session_id,
                global_types::SessionStatus::Stopped,
                None,
            )
            .await;
            Err(anyhow::anyhow!(
                "persist_status_transition failed for review spawn: {e}"
            ))
        }
    }
}

async fn build_codex_review_prompt(
    item: &Task,
    trigger: &str,
    worker_contexts_text: String,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<String> {
    let evidence = super::captain_review_evidence::compute_evidence_listing(pool, item).await;
    let knowledge_base = read_knowledge_base().await;
    let problem_statement = problem_statement(item);
    let trigger_flags = trigger_flags(trigger);
    let mut vars: FxHashMap<&str, String> = FxHashMap::default();
    vars.insert("trigger", trigger.to_string());
    vars.insert("title", item.title.clone());
    vars.insert("item_id", item.id.to_string());
    vars.insert("worker_contexts", worker_contexts_text);
    vars.insert("knowledge_base", knowledge_base);
    vars.insert("evidence_images", evidence.listing.clone());
    vars.insert("problem_statement", problem_statement);
    vars.insert("evidence_files", evidence.listing.clone());
    vars.insert("work_summary", evidence.work_summary.clone());
    vars.insert("intervention_count", item.intervention_count.to_string());
    vars.insert(
        bool_key("has_screenshot"),
        bool_value(evidence.has_screenshot),
    );
    vars.insert(
        bool_key("has_recording"),
        bool_value(evidence.has_recording),
    );
    vars.insert(
        bool_key("has_before_fix"),
        bool_value(evidence.has_before_fix),
    );
    vars.insert(
        bool_key("has_after_fix"),
        bool_value(evidence.has_after_fix),
    );
    vars.insert(
        bool_key("has_cannot_reproduce"),
        bool_value(evidence.has_cannot_reproduce),
    );
    vars.insert(
        bool_key("has_before_screenshot"),
        bool_value(evidence.has_before_screenshot),
    );
    vars.insert(
        bool_key("has_after_screenshot"),
        bool_value(evidence.has_after_screenshot),
    );
    vars.insert(
        bool_key("has_after_recording"),
        bool_value(evidence.has_after_recording),
    );
    vars.insert("is_no_pr", if item.no_pr { "true" } else { "" }.into());
    vars.insert(
        "is_bug_fix",
        if item.is_bug_fix { "true" } else { "" }.into(),
    );
    for (key, flag) in &trigger_flags {
        vars.insert(key.as_str(), flag.clone());
    }
    settings::render_prompt("captain_review", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("render captain_review prompt: {e}"))
}

async fn read_knowledge_base() -> String {
    let knowledge_path = global_infra::paths::state_dir().join("knowledge.md");
    match tokio::fs::read_to_string(&knowledge_path).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            tracing::warn!(module = "captain", error = %e, "failed to read knowledge.md");
            String::new()
        }
    }
}

fn problem_statement(item: &Task) -> String {
    let mut parts = vec![item.title.clone()];
    if let Some(ref ctx) = item.context {
        parts.push(ctx.clone());
    }
    if let Some(ref prompt) = item.original_prompt {
        parts.push(prompt.clone());
    }
    parts.join("\n\n")
}

fn trigger_flags(trigger: &str) -> Vec<(String, String)> {
    super::captain_review::TRIGGERS
        .iter()
        .map(|name| {
            let key = format!("is_{name}");
            let flag = if trigger == *name { "true" } else { "" }.to_string();
            (key, flag)
        })
        .collect()
}

fn bool_key(key: &'static str) -> &'static str {
    key
}

fn bool_value(value: bool) -> String {
    if value { "true" } else { "false" }.into()
}

fn rollback_review_spawn(
    item: &mut Task,
    prev_status: ItemStatus,
    saved_last_activity: Option<String>,
) {
    lifecycle::restore_status(item, prev_status);
    item.captain_review_trigger = None;
    item.session_ids.review = None;
    item.last_activity_at = saved_last_activity;
}
