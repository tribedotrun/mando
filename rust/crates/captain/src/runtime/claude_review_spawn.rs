//! Claude adapter for captain review sessions.

use std::panic::AssertUnwindSafe;

use anyhow::Result;
use futures::FutureExt;
use rustc_hash::FxHashMap;
use settings::CaptainWorkflow;
use tracing::{info, warn};

use crate::service::lifecycle;
use crate::{ItemStatus, Task, TimelineEventPayload};

use super::captain_review::{codex_verdict_output_schema, TRIGGERS};
use super::captain_review_helpers::escaped_title;
use super::notify::Notifier;

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = "claude", task_id = item.id, trigger))]
pub(super) async fn spawn_claude_review(
    item: &mut Task,
    trigger: &str,
    db_status: Option<&str>,
    cwd: std::path::PathBuf,
    parsed_trigger: crate::ReviewTrigger,
    worker_contexts_text: String,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    // All fallible operations succeeded, now commit state changes.
    // Use db_status if provided (callers that called reset_review_retry before
    // us pass the pre-reset status), otherwise use the item's current status.
    let guard_status = db_status
        .map(|s| s.to_string())
        .unwrap_or_else(|| item.status.as_str().to_string());
    let prev_status = item.status;
    let saved_last_activity = item.last_activity_at.clone();
    lifecycle::apply_transition(item, ItemStatus::CaptainReviewing)?;
    item.captain_review_trigger = Some(parsed_trigger);
    item.last_activity_at = Some(global_types::now_rfc3339());
    let task_id = item.id.to_string();
    let task_id_num = item.id;
    let session_id = global_infra::uuid::Uuid::v4().to_string();
    item.session_ids.review = Some(session_id.clone());

    let event = crate::TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!("Captain review started (trigger: {trigger})"),
        data: TimelineEventPayload::CaptainReviewStarted {
            trigger: trigger.to_string(),
            session_id: session_id.clone(),
        },
    };
    match crate::io::queries::tasks::persist_status_transition(pool, item, &guard_status, &event)
        .await
    {
        Ok(true) => {
            notifier
                .normal(&format!(
                    "\u{1f50d} Captain reviewing <b>{}</b> (trigger: {trigger})",
                    escaped_title(item),
                ))
                .await;
        }
        Ok(false) => {
            // Roll back all speculative in-memory mutations so the end-of-tick
            // write-back doesn't persist a never-spawned review session.
            lifecycle::restore_status(item, prev_status);
            item.captain_review_trigger = None;
            item.session_ids.review = None;
            item.last_activity_at = saved_last_activity.clone();
            tracing::info!(
                module = "captain",
                item_id = item.id,
                "review spawn transition already applied"
            );
            return Ok(());
        }
        Err(e) => {
            lifecycle::restore_status(item, prev_status);
            item.captain_review_trigger = None;
            item.session_ids.review = None;
            item.last_activity_at = saved_last_activity;
            return Err(anyhow::anyhow!(
                "persist_status_transition failed for review spawn: {e}"
            ));
        }
    }

    // Pick a credential for this review session so it goes through
    // multi-credential load balancing when credentials are configured.
    let credential = super::tick_spawn::pick_credential(pool, None).await;
    let cred_id = credential.as_ref().map(|c| c.0);

    // Log "running" session entry eagerly so (a) cancel can find it
    // immediately and (b) timeline never references a missing session.
    if let Err(e) = crate::io::headless_cc::log_running_session(
        pool,
        &session_id,
        &cwd,
        "captain-review-async",
        "",
        Some(item.id),
        false,
        cred_id,
    )
    .await
    {
        warn!(module = "captain", %session_id, %e, "failed to log running session");
    }

    // Clone data needed by the spawned task. Pre-stringify values that don't
    // depend on async I/O so they're computed once per review instead of on
    // every spawn closure run.
    let trigger_str = trigger.to_string();
    let item_title = item.title.clone();
    let item_id = item.id.to_string();
    let item_no_pr = item.no_pr;
    let item_is_bug_fix = item.is_bug_fix;

    // Build problem statement from task metadata.
    let problem_statement = {
        let mut parts = vec![item.title.clone()];
        if let Some(ref ctx) = item.context {
            parts.push(ctx.clone());
        }
        if let Some(ref prompt) = item.original_prompt {
            parts.push(prompt.clone());
        }
        parts.join("\n\n")
    };

    // Build evidence file listing from DB artifacts and detect evidence types.
    let evidence = super::captain_review_evidence::compute_evidence_listing(pool, item).await;
    let evidence_file_listing = evidence.listing;
    let work_summary_content = evidence.work_summary;
    let has_screenshot = evidence.has_screenshot;
    let has_recording = evidence.has_recording;
    let has_before_fix = evidence.has_before_fix;
    let has_after_fix = evidence.has_after_fix;
    let has_cannot_reproduce = evidence.has_cannot_reproduce;
    let has_before_screenshot = evidence.has_before_screenshot;
    let has_after_screenshot = evidence.has_after_screenshot;
    let has_after_recording = evidence.has_after_recording;
    let intervention_count_str = item.intervention_count.to_string();
    let trigger_flags: Vec<(String, String)> = TRIGGERS
        .iter()
        .map(|name| {
            let key = format!("is_{name}");
            let flag = if trigger_str == *name {
                "true".to_string()
            } else {
                String::new()
            };
            (key, flag)
        })
        .collect();
    let timeout = workflow.agent.captain_review_timeout_s;
    let prompts = workflow.prompts.clone();
    let captain_model = workflow.models.captain.clone();
    let cc_max_retries = workflow.agent.cc_max_retries;
    let pool = pool.clone();
    let review_notifier = notifier.fork();

    let session_id_for_panic = session_id.clone();
    // TRACKED: detached captain-review CC session. Not registered with the
    // gateway's TaskTracker because mando-captain is a library crate and has
    // no dependency on AppState. On shutdown the external CC process is killed
    // via the pid registry; this task writes its final verdict to the stream
    // file which persists across restarts, so no in-memory state is lost.
    tokio::spawn(async move {
        let result = AssertUnwindSafe(async move {
        // Evidence is now managed by the CLI (mando todo evidence) and
        // served from the DB. The evidence_file_listing and work_summary_content
        // were pre-computed from DB before the spawn.
        let evidence_listing = evidence_file_listing.clone();

        // Load knowledge base.
        let knowledge_path = global_infra::paths::state_dir().join("knowledge.md");
        let knowledge_base = match tokio::fs::read_to_string(&knowledge_path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                warn!(module = "captain", error = %e, "failed to read knowledge.md");
                String::new()
            }
        };

        // Assemble template variables. Values that don't depend on async I/O
        // were pre-computed before `tokio::spawn`; we only insert references
        // here. `FxHashMap` keyed by `&str` with owned `String` values gives
        // the hot path a faster hasher without fighting the borrow checker over
        // per-call-site lifetimes.
        let mut vars: FxHashMap<&str, String> = FxHashMap::default();
        vars.insert("trigger", trigger_str.clone());
        vars.insert("title", item_title.clone());
        vars.insert("item_id", item_id.clone());
        vars.insert("worker_contexts", worker_contexts_text.clone());
        vars.insert("knowledge_base", knowledge_base.clone());
        vars.insert("evidence_images", evidence_listing.clone());
        vars.insert("problem_statement", problem_statement.clone());
        vars.insert("evidence_files", evidence_file_listing.clone());
        vars.insert("work_summary", work_summary_content.clone());
        vars.insert("intervention_count", intervention_count_str.clone());
        vars.insert(
            "has_screenshot",
            if has_screenshot { "true" } else { "false" }.into(),
        );
        vars.insert(
            "has_recording",
            if has_recording { "true" } else { "false" }.into(),
        );
        // Typed bug-fix gates. When `is_bug_fix` is also true, the reviewer
        // prompt routes deterministically: missing before/after kind tags
        // produce a nudge with hardcoded text, so the LLM cannot accidentally
        // ship a fix without paired before/after evidence.
        vars.insert(
            "has_before_fix",
            if has_before_fix { "true" } else { "false" }.into(),
        );
        vars.insert(
            "has_after_fix",
            if has_after_fix { "true" } else { "false" }.into(),
        );
        // Worker-typed signal that the bug cannot be triggered. The reviewer
        // prompt is wired to escalate to the human deterministically when this
        // flag is set, instead of nudging for before/after evidence that
        // doesn't exist.
        vars.insert(
            "has_cannot_reproduce",
            if has_cannot_reproduce { "true" } else { "false" }.into(),
        );
        // Universal UI evidence gates. Each intersects extension AND
        // `--kind` tag so a `--kind before` terminal log cannot satisfy
        // the UI before-screenshot rule. Recording is required only on
        // the after side.
        vars.insert(
            "has_before_screenshot",
            if has_before_screenshot { "true" } else { "false" }.into(),
        );
        vars.insert(
            "has_after_screenshot",
            if has_after_screenshot { "true" } else { "false" }.into(),
        );
        vars.insert(
            "has_after_recording",
            if has_after_recording { "true" } else { "false" }.into(),
        );
        // no_pr tasks have no diff, no PR, no merge step — the worker
        // transcript and any DB-backed evidence is the entire deliverable.
        // The prompt uses this flag to relax the screenshot + recording gate
        // that only applies to PR review.
        vars.insert(
            "is_no_pr",
            if item_no_pr { "true" } else { "" }.into(),
        );
        // Threaded into the bug-fix evidence rule in `captain_review`. When
        // set, captain demands BOTH before-state (showing the bug) and
        // after-state (showing the fix) evidence; nudges if either is missing.
        vars.insert(
            "is_bug_fix",
            if item_is_bug_fix { "true" } else { "" }.into(),
        );
        for (key, flag) in &trigger_flags {
            vars.insert(key.as_str(), flag.clone());
        }

        let prompt = match settings::render_prompt("captain_review", &prompts, &vars) {
            Ok(p) => p,
            Err(e) => {
                warn!(module = "captain", %session_id, %e, "failed to render captain review prompt");
                let stream_path = global_infra::paths::stream_path_for_session(&session_id);
                global_claude::write_error_result(
                    &stream_path,
                    &format!("failed to render captain review prompt: {e}"),
                );
                return;
            }
        };

        let builder = global_claude::CcConfig::builder()
            .model(&captain_model)
            .timeout(timeout)
            .caller("captain-review-async")
            .task_id(&task_id)
            .cwd(cwd.clone())
            .session_id(session_id.clone())
            .allowed_tools(vec!["Read".into(), "Bash".into()])
            .disallowed_tools(vec!["Agent".into()])
            .json_schema(codex_verdict_output_schema(&trigger_str).0);
        let config = global_claude::with_credential(builder, &credential).build();

        let sid_for_hook = session_id.clone();
        match global_claude::CcOneShot::run_with_retry_pid_hook(
            &prompt,
            config,
            cc_max_retries,
            |pid| {
                if let Err(e) = crate::io::pid_registry::register(&sid_for_hook, pid) {
                    warn!(module = "captain", sid = %sid_for_hook, %e, "pid_registry register failed");
                }
            },
        )
        .await
        {
            Ok(result) => {
                info!(module = "captain", %session_id, "captain review CC completed");
                if let Err(e) = crate::io::pid_registry::unregister(&session_id) {
                    warn!(module = "captain", %session_id, %e, "pid_registry unregister failed");
                }
                let cred_id = sessions_db::get_credential_id(&pool, &session_id)
                    .await
                    .unwrap_or(None);
                review_notifier
                    .check_rate_limit(&result, &pool, cred_id)
                    .await;
                if let Err(e) = crate::io::headless_cc::log_cc_result(
                    &pool,
                    &result,
                    &cwd,
                    "captain-review-async",
                    Some(task_id_num),
                )
                .await {
                    warn!(module = "captain", %session_id, %e, "log_cc_result failed");
                }
            }
            Err(e) => {
                warn!(module = "captain", %session_id, %e, "captain review CC failed");
                if let Err(e2) = crate::io::pid_registry::unregister(&session_id) {
                    warn!(module = "captain", %session_id, %e2, "pid_registry unregister failed");
                }
                // Write a synthetic error result so check_review() finds it on
                // the next tick instead of waiting for the full timeout.
                let stream_path = global_infra::paths::stream_path_for_session(&session_id);
                global_claude::write_error_result(
                    &stream_path,
                    &format!("captain review CC process failed: {e}"),
                );
                let error_text = format!("{e}");
                let api_error_status = e.api_error_status();
                if let Err(e2) = crate::io::headless_cc::log_cc_failure(
                    &pool,
                    &session_id,
                    &cwd,
                    "captain-review-async",
                    Some(task_id_num),
                    Some(&error_text),
                    api_error_status,
                )
                .await {
                    warn!(module = "captain", %session_id, %e2, "log_cc_failure failed");
                }
            }
        }
        })
        .catch_unwind()
        .await;

        if let Err(panic) = result {
            tracing::error!(
                module = "captain",
                session_id = %session_id_for_panic,
                "captain review spawn panicked: {:?}",
                panic
            );
            let stream_path = global_infra::paths::stream_path_for_session(&session_id_for_panic);
            global_claude::write_error_result(
                &stream_path,
                &format!("captain review spawn panicked: {:?}", panic),
            );
        }
    });

    Ok(())
}
