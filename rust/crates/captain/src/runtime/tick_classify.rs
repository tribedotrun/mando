//! Worker classification and health-state updates. Extracted from tick.rs phase 2.

use crate::{Action, Task, WorkerContext};
use anyhow::Result;
use settings::CaptainWorkflow;

use crate::io::{health_store, health_store::HealthState};

/// Result of classifying all worker contexts in a tick.
pub(super) struct ClassifyResult {
    /// Actions to execute (live mode).
    pub actions_to_execute: Vec<Action>,
    /// Actions collected in dry-run mode.
    pub dry_actions: Vec<Action>,
}

/// Classify each worker context and update health state with current values.
///
/// For each worker, this:
/// 1. Looks up its task and stream result.
/// 2. Runs deterministic classification.
/// 3. Updates health state (cpu_time_s, cwd).
pub(super) fn classify_and_update_health(
    worker_contexts: &[WorkerContext],
    items: &[Task],
    health_state: &mut HealthState,
    workflow: &CaptainWorkflow,
    dry_run: bool,
) -> Result<ClassifyResult> {
    let mut actions_to_execute = Vec::new();
    let mut dry_actions = Vec::new();
    // One matcher per tick — lowercases and owns the configured rule list so
    // per-worker `detect()` calls stay cheap.
    let symptoms = global_claude::StreamSymptomMatcher::new(workflow.stream_symptoms.clone());

    for ctx in worker_contexts {
        // Look up the task for this worker.
        let item_ref = items
            .iter()
            .find(|it| it.worker.as_deref() == Some(&ctx.session_name));

        // Get stream result for this worker via session_id.
        let cc_sid = item_ref.and_then(|it| it.session_ids.worker.as_deref());
        let stream_path = item_ref
            .zip(cc_sid)
            .map(|(item, sid)| super::agent_runtime::stream_path(item.provider, sid));
        let stream_result = stream_path
            .as_deref()
            .and_then(global_claude::get_stream_result);
        let stream_clean = stream_result.as_ref().map(global_claude::is_clean_result);
        let has_broken_session = stream_path
            .as_deref()
            .is_some_and(global_claude::stream_has_broken_session);

        let action = crate::service::deterministic::classify_worker(
            ctx,
            item_ref,
            stream_clean,
            has_broken_session,
            stream_path.as_deref(),
            &workflow.nudges,
            &symptoms,
            workflow.agent.worker_timeout_s,
            workflow.agent.stale_threshold_s,
            workflow.agent.max_interventions,
            workflow.agent.no_pr_min_active_s,
        )?;

        if dry_run {
            dry_actions.push(action);
        } else {
            actions_to_execute.push(action);
        }

        // Update health state with current context values.
        if let Some(cpu) = ctx.cpu_time_s {
            health_store::set_health_field(
                health_state,
                &ctx.session_name,
                "cpu_time_s",
                serde_json::json!(cpu),
            );
        }

        // Persist worker CWD from the item's worktree field.
        if let Some(wt) = item_ref.and_then(|it| it.worktree.as_deref()) {
            health_store::set_health_field(
                health_state,
                &ctx.session_name,
                "cwd",
                serde_json::json!(wt),
            );
        }
    }

    Ok(ClassifyResult {
        actions_to_execute,
        dry_actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolate_data_dir() -> (std::path::PathBuf, global_infra::EnvVarGuard) {
        let dir = std::env::temp_dir().join(format!(
            "mando-tick-classify-{}",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", &dir);
        (dir, guard)
    }

    #[tokio::test]
    async fn codex_classification_reads_codex_derived_stream_result() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (_dir, _guard) = isolate_data_dir();
        let sid = "codex-derived-classify";
        let stream_path =
            crate::runtime::agent_runtime::stream_path(global_types::TaskProvider::Codex, sid);
        std::fs::create_dir_all(stream_path.parent().unwrap()).unwrap();
        std::fs::write(
            &stream_path,
            concat!(
                r#"{"type":"system","subtype":"init"}"#,
                "\n",
                r#"{"type":"result","subtype":"success","is_error":false}"#,
                "\n"
            ),
        )
        .unwrap();
        assert!(
            !global_infra::paths::stream_path_for_session(sid).exists(),
            "test must prove classification does not require the Claude stream path"
        );

        let mut item = Task::new("Codex classify proof");
        item.provider = global_types::TaskProvider::Codex;
        item.worker = Some("codex-worker".into());
        item.session_ids.worker = Some(sid.into());
        item.status = crate::ItemStatus::InProgress;
        item.no_pr = true;

        let ctx = WorkerContext {
            session_name: "codex-worker".into(),
            item_title: item.title.clone(),
            status: "in-progress".into(),
            stream_tail: "Codex produced enough no-PR output to pass review gates.".into(),
            seconds_active: workflow_no_pr_min_active_s(),
            ..WorkerContext::default()
        };
        let result = classify_and_update_health(
            &[ctx],
            &[item],
            &mut HealthState::default(),
            &CaptainWorkflow::compiled_default(),
            true,
        )
        .unwrap();

        assert_eq!(result.dry_actions.len(), 1);
        assert_eq!(
            result.dry_actions[0].action,
            crate::ActionKind::CaptainReview
        );
        assert_eq!(result.dry_actions[0].reason.as_deref(), Some("gates_pass"));
    }

    fn workflow_no_pr_min_active_s() -> f64 {
        settings::CaptainWorkflow::compiled_default()
            .agent
            .no_pr_min_active_s
            .as_secs_f64()
            + 1.0
    }
}
