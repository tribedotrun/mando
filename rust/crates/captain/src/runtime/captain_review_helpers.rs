//! Shared helpers for captain review: prompt assembly and verdict application.

use anyhow::Result;
use rustc_hash::FxHashMap;
use tracing::warn;

use crate::{ReviewTrigger, Task};
use settings::CaptainWorkflow;

use sqlx::SqlitePool;

pub(crate) fn escaped_title(item: &Task) -> String {
    global_infra::html::escape_html(&item.title)
}

/// Status the persist guard compares against.
///
/// Callers that already ran `reset_review_retry` pass the pre-reset status;
/// everyone else guards on the task's current status.
pub(super) fn review_guard_status(item: &Task, db_status: Option<&str>) -> String {
    db_status
        .map(str::to_string)
        .unwrap_or_else(|| item.status.as_str().to_string())
}

/// The timeline event both review adapters persist alongside the transition.
pub(super) fn review_started_event(trigger: &str, session_id: &str) -> crate::TimelineEvent {
    crate::TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!("Captain review started (trigger: {trigger})"),
        data: crate::TimelineEventPayload::CaptainReviewStarted {
            trigger: trigger.to_string(),
            session_id: session_id.to_string(),
        },
    }
}

#[tracing::instrument(skip_all, fields(task_id = item.id, trigger))]
pub(super) async fn notify_review_started(
    notifier: &super::notify::Notifier,
    item: &Task,
    trigger: &str,
) {
    notifier
        .normal(&format!(
            "\u{1f50d} Captain reviewing <b>{}</b> (trigger: {trigger})",
            escaped_title(item),
        ))
        .await;
}

/// Undo the speculative in-memory mutations a review spawn made, so the
/// end-of-tick write-back cannot persist a review session that never started.
pub(super) fn rollback_review_spawn(
    item: &mut Task,
    prev_status: crate::ItemStatus,
    saved_last_activity: Option<String>,
) {
    crate::service::lifecycle::restore_status(item, prev_status);
    item.captain_review_trigger = None;
    item.session_ids.review = None;
    item.last_activity_at = saved_last_activity;
}

/// The complete variable set the `captain_review` template consumes.
///
/// One assembly point for both provider adapters, so the Claude and Codex
/// spawn paths cannot drift apart and a template edit has exactly one place
/// to land. Every key inserted here is referenced by `prompts.captain_review`
/// in `captain-workflow.yaml`, and nothing else is inserted — pinned by
/// `captain_review_render_tests::declared_vars_exactly_match_the_template`,
/// which diffs these keys against the template's own references.
pub(super) fn review_template_vars(
    item: &Task,
    trigger: &str,
    parsed_trigger: ReviewTrigger,
    worker_contexts_text: String,
    work_summary: String,
    knowledge_base: String,
    evidence_images: String,
) -> FxHashMap<&'static str, String> {
    let mut vars: FxHashMap<&'static str, String> = FxHashMap::default();
    vars.insert("trigger", trigger.to_string());
    vars.insert("problem_statement", problem_statement(item));
    vars.insert("worker_contexts", worker_contexts_text);
    vars.insert("work_summary", work_summary);
    vars.insert("knowledge_base", knowledge_base);
    vars.insert("evidence_images", evidence_images);
    vars.insert("is_no_pr", flag(item.no_pr));
    vars.insert("is_bug_fix", flag(item.is_bug_fix));
    // Derived from the parsed trigger rather than a string allowlist so a new
    // `ReviewTrigger` variant can never silently render as "not a CI failure".
    vars.insert(
        "is_ci_failure",
        flag(parsed_trigger == ReviewTrigger::CiFailure),
    );
    vars.insert("workpad_path", review_workpad_path(item.id));
    vars
}

/// Render the `captain_review` prompt for `item`.
///
/// Both provider adapters call this *before* committing the task to
/// `CaptainReviewing`: a render failure must return `Err` with the task
/// untouched rather than stranding it in a review state with no session.
#[tracing::instrument(skip_all, fields(task_id = item.id, trigger))]
pub(super) async fn build_review_prompt(
    item: &Task,
    trigger: &str,
    parsed_trigger: ReviewTrigger,
    worker_contexts_text: String,
    workflow: &CaptainWorkflow,
    pool: &SqlitePool,
) -> Result<String> {
    let evidence = super::captain_review_evidence::compute_evidence_listing(pool, item).await;
    let knowledge_base = read_knowledge_base().await;
    let vars = review_template_vars(
        item,
        trigger,
        parsed_trigger,
        worker_contexts_text,
        evidence.work_summary,
        knowledge_base,
        evidence.listing,
    );

    settings::render_prompt("captain_review", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("render captain_review prompt: {e}"))
}

/// Jinja-truthy marker. Empty string renders falsy under
/// `settings::render_prompt`'s scalar coercion; `"true"` becomes a real bool.
fn flag(value: bool) -> String {
    if value { "true" } else { "" }.to_string()
}

/// Data-dir workpad the reviewer cites for no-PR tasks:
/// `{data_dir}/plans/<task-id>/workpad.md`.
///
/// Read-only on purpose — the worker spawner owns creating this file
/// (`spawner_prompt::ensure_workpad_path`); a review must never manufacture an
/// empty workpad and make a missing deliverable look present.
fn review_workpad_path(task_id: i64) -> String {
    global_infra::paths::data_dir()
        .join("plans")
        .join(task_id.to_string())
        .join("workpad.md")
        .display()
        .to_string()
}

async fn read_knowledge_base() -> String {
    let knowledge_path = global_infra::paths::state_dir().join("knowledge.md");
    match tokio::fs::read_to_string(&knowledge_path).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            warn!(module = "captain", error = %e, "failed to read knowledge.md");
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

/// Inline resume of a worker process with feedback. Shared by `nudge` and
/// `reset_budget` verdict handlers. Kills old process, checks for broken
/// stream, resumes with feedback, updates health state and session log.
///
/// Returns `true` if the worker was successfully resumed.
#[tracing::instrument(skip_all)]
pub(super) async fn inline_resume_worker(
    item: &Task,
    feedback: &str,
    workflow: &CaptainWorkflow,
    pool: &SqlitePool,
) -> bool {
    let (Some(w), Some(sid), Some(wt)) = (&item.worker, &item.session_ids.worker, &item.worktree)
    else {
        warn!(
            module = "captain",
            item_id = item.id,
            "verdict resume has no worker/session/worktree; next tick will handle"
        );
        return false;
    };

    let wt_path = global_infra::paths::expand_tilde(wt);
    let current_pid = crate::io::pid_lookup::resolve_pid(sid, w).unwrap_or(crate::Pid::new(0));
    let delivery = match super::agent_runtime::nudge_worker(
        pool,
        item,
        w,
        &wt_path,
        feedback,
        sid,
        &workflow.models.worker,
        workflow,
        current_pid,
    )
    .await
    {
        Ok(super::agent_runtime::AgentNudgeOutcome::Delivered(delivery)) => delivery,
        Ok(super::agent_runtime::AgentNudgeOutcome::BrokenSession { alert }) => {
            warn!(module = "captain", worker = %w, %sid, %alert, "verdict skipped resume; stream is broken");
            return false;
        }
        Err(e) => {
            warn!(module = "captain", worker = %w, error = %e,
                "verdict resume failed; next tick will retry");
            return false;
        }
    };
    {
        // Health-state bookkeeping must not abort: the worker is already
        // running. Degrade gracefully on failure instead of double-resuming.
        let health_path = crate::config::worker_health_path();
        match crate::io::health_store::load_health_state(&health_path) {
            Ok(mut hstate) => {
                crate::io::health_store::set_health_field(
                    &mut hstate,
                    w,
                    "pid",
                    serde_json::json!(delivery.pid),
                );
                crate::io::health_store::set_health_field(
                    &mut hstate,
                    w,
                    "stream_size_at_spawn",
                    serde_json::json!(delivery.stream_size_before),
                );
                if let Err(e) = crate::io::health_store::save_health_state(&health_path, &hstate) {
                    warn!(module = "captain", worker = %w, error = %e,
                            "failed to persist health after verdict resume");
                }
            }
            Err(e) => {
                warn!(module = "captain", worker = %w, error = %e,
                        "failed to load health state after verdict resume; skipping bookkeeping");
            }
        }
        true
    }
}
