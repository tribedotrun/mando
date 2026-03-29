//! Async, non-blocking captain review sessions.
//!
//! When the classifier decides an item needs CC review, the captain:
//! 1. Spawns a headless CC session (non-blocking)
//! 2. Sets item status to CaptainReviewing
//! 3. On subsequent ticks, polls for completion
//! 4. Applies the verdict (ship/nudge/respawn/escalate/retry)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;

use super::notify::Notifier;
use super::review_phase;
use super::timeline_emit;
use crate::io::{evidence, process_manager};

use super::captain_review_verdict::escaped_title;
pub use super::captain_review_verdict::{apply_verdict, handle_review_error};

/// Structured verdict from a captain review CC session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptainVerdict {
    pub action: String,
    pub feedback: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
}

fn is_verdict_allowed(trigger: &str, action: &str) -> bool {
    match trigger {
        "gates_pass" => matches!(action, "ship" | "nudge" | "escalate"),
        "timeout" => matches!(action, "nudge" | "escalate"),
        "broken_session" => matches!(action, "respawn" | "escalate"),
        "budget_exhausted" => matches!(action, "escalate"),
        "clarifier_fail" => matches!(action, "retry_clarifier" | "escalate"),
        "rebase_fail" => matches!(action, "nudge" | "escalate"),
        "ci_failure" => matches!(action, "nudge" | "escalate"),
        _ => false,
    }
}

/// All possible verdict actions across all triggers.
const ALL_VERDICT_ACTIONS: &[&str] = &["ship", "nudge", "escalate", "respawn", "retry_clarifier"];

/// JSON Schema for the CaptainVerdict structured output.
fn verdict_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ALL_VERDICT_ACTIONS,
                "description": "The verdict action — must be one of the allowed values"
            },
            "feedback": {
                "type": "string",
                "description": "Specific instructions for worker or summary for human"
            },
            "report": {
                "type": "string",
                "description": "CTO-level report, required for escalate"
            }
        },
        "required": ["action", "feedback"]
    })
}

/// Spawn a captain review for an item. Sets status to CaptainReviewing.
///
/// The CC session runs async (tokio::spawn) — not awaited here.
pub(crate) async fn spawn_review(
    item: &mut Task,
    trigger: &str,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    // Resolve CWD before mutating item state — if this fails,
    // the item stays in its current status and the caller can retry or escalate.

    let cwd = item
        .worktree
        .as_deref()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            config
                .captain
                .projects
                .values()
                .next()
                .map(|p| std::path::PathBuf::from(&p.path))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no CWD for captain review: item has no worktree and no projects configured"
            )
        })?;

    // All fallible operations succeeded — now commit state changes.
    item.status = ItemStatus::CaptainReviewing;
    item.captain_review_trigger = trigger.parse().ok();
    item.last_activity_at = Some(mando_types::now_rfc3339());
    item.retry_count = 0;

    // --- Gather worker context (PR data, stream tail, process status) ---
    let (ctx, worker_contexts_text) = review_phase::build_single_context(item, config).await;
    let pr_body_for_evidence = ctx.pr_body.clone();
    let worker_name_owned = item.worker.as_deref().unwrap_or("unknown").to_string();

    let task_id = item.best_id();
    let session_id = mando_uuid::Uuid::v4().to_string();
    item.session_ids.review = Some(session_id.clone());

    timeline_emit::emit_for_task(
        item,
        TimelineEventType::CaptainReviewStarted,
        &format!("Captain review started (trigger: {trigger})"),
        serde_json::json!({ "trigger": trigger, "session_id": session_id }),
        pool,
    )
    .await;
    notifier
        .normal(&format!(
            "\u{1f50d} Captain reviewing <b>{}</b> (trigger: {trigger})",
            escaped_title(item),
        ))
        .await;

    // Clone data needed by the spawned task.
    let trigger_str = trigger.to_string();
    let item_title = item.title.clone();
    let item_id = item.best_id();
    let intervention_count_val = item.intervention_count;
    let timeout_s = workflow.agent.captain_review_timeout_s;
    let prompts = workflow.prompts.clone();
    let captain_model = workflow.models.captain.clone();
    let pool = pool.clone();
    let review_notifier = notifier.fork();

    tokio::spawn(async move {
        // Download evidence images from PR body.
        let work_dir = mando_config::state_dir().join("captain-evidence");
        let worker_dir = work_dir.join(&worker_name_owned);
        let evidence_paths = evidence::download_evidence(&pr_body_for_evidence, &worker_dir).await;
        let evidence_listing = if evidence_paths.is_empty() {
            String::new()
        } else {
            let mut listing = format!("\n**{}**:\n", worker_name_owned);
            for path in &evidence_paths {
                listing.push_str(&format!("- {}\n", path.display()));
            }
            listing
        };

        // Load knowledge base.
        let knowledge_path = mando_config::state_dir().join("knowledge.md");
        let knowledge_base = match tokio::fs::read_to_string(&knowledge_path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                warn!(module = "captain", error = %e, "failed to read knowledge.md");
                String::new()
            }
        };

        // Build template variables.
        let intervention_count_str = intervention_count_val.to_string();
        let is_gates_pass = if trigger_str == "gates_pass" {
            "true"
        } else {
            ""
        };
        let is_timeout = if trigger_str == "timeout" { "true" } else { "" };
        let is_broken_session = if trigger_str == "broken_session" {
            "true"
        } else {
            ""
        };
        let is_budget_exhausted = if trigger_str == "budget_exhausted" {
            "true"
        } else {
            ""
        };
        let is_clarifier_fail = if trigger_str == "clarifier_fail" {
            "true"
        } else {
            ""
        };
        let is_rebase_fail = if trigger_str == "rebase_fail" {
            "true"
        } else {
            ""
        };
        let is_ci_failure = if trigger_str == "ci_failure" {
            "true"
        } else {
            ""
        };

        let mut vars = std::collections::HashMap::new();
        vars.insert("trigger", trigger_str.as_str());
        vars.insert("title", item_title.as_str());
        vars.insert("item_id", item_id.as_str());
        vars.insert("worker_contexts", worker_contexts_text.as_str());
        vars.insert("knowledge_base", knowledge_base.as_str());
        vars.insert("evidence_images", evidence_listing.as_str());
        vars.insert("intervention_count", intervention_count_str.as_str());
        vars.insert("is_gates_pass", is_gates_pass);
        vars.insert("is_timeout", is_timeout);
        vars.insert("is_broken_session", is_broken_session);
        vars.insert("is_budget_exhausted", is_budget_exhausted);
        vars.insert("is_clarifier_fail", is_clarifier_fail);
        vars.insert("is_rebase_fail", is_rebase_fail);
        vars.insert("is_ci_failure", is_ci_failure);

        let prompt = match mando_config::render_prompt("captain_review", &prompts, &vars) {
            Ok(p) => p,
            Err(e) => {
                warn!(module = "captain", %session_id, %e, "failed to render captain review prompt");
                return;
            }
        };

        let config = mando_cc::CcConfig::builder()
            .model(&captain_model)
            .timeout(std::time::Duration::from_secs(timeout_s))
            .caller("captain-review-async")
            .task_id(&task_id)
            .cwd(cwd.clone())
            .session_id(session_id.clone())
            .allowed_tools(vec!["Read".into()])
            .json_schema(verdict_json_schema())
            .build();

        match mando_cc::CcOneShot::run(&prompt, config).await {
            Ok(result) => {
                info!(module = "captain", %session_id, "captain review CC completed");
                review_notifier.check_rate_limit(&result).await;
                crate::io::headless_cc::log_cc_result(
                    &pool,
                    &result,
                    &cwd,
                    "captain-review-async",
                    &task_id,
                )
                .await;
            }
            Err(e) => {
                warn!(module = "captain", %session_id, %e, "captain review CC failed");
                crate::io::headless_cc::log_cc_failure(
                    &pool,
                    &session_id,
                    &cwd,
                    "captain-review-async",
                    &task_id,
                )
                .await;
            }
        }
    });

    Ok(())
}

/// Check if a captain review has completed. Returns the verdict if done.
pub(crate) fn check_review(item: &Task) -> Option<CaptainVerdict> {
    let session_id = item.session_ids.review.as_deref()?;
    let stream_path = mando_config::stream_path_for_session(session_id);
    let result = mando_cc::get_stream_result(&stream_path)?;

    // Try structured_output first (populated when --json-schema was used).
    if let Some(so) = result.get("structured_output").filter(|v| !v.is_null()) {
        match serde_json::from_value::<CaptainVerdict>(so.clone()) {
            Ok(verdict) => return Some(validate_verdict(verdict, item)),
            Err(e) => {
                let raw_preview: String = so.to_string().chars().take(300).collect();
                warn!(module = "captain", %e, %session_id, raw = %raw_preview,
                    "structured_output present but failed to parse, trying fallbacks");
            }
        }
    }

    // Fall back to result text field.
    let mut verdict_text = result
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // If result field is empty, recover from the last assistant text block.
    if verdict_text.is_empty() {
        if let Some(text) = process_manager::get_last_assistant_text(&stream_path) {
            warn!(module = "captain", %session_id,
                "check_review: result field empty, recovered from last assistant text");
            verdict_text = text;
        } else {
            // Session completed but produced no extractable verdict — escalate.
            warn!(module = "captain", %session_id,
                "check_review: session completed but all extraction paths empty, escalating");
            return Some(CaptainVerdict {
                action: "escalate".into(),
                feedback: "Captain review session completed but produced no extractable verdict"
                    .into(),
                report: Some(
                    "Captain review session completed but all extraction paths \
                     (structured_output, result text, last assistant text) were empty. \
                     The CC session may have failed silently or produced no usable output. \
                     Manual review required."
                        .into(),
                ),
            });
        }
    }

    match serde_json::from_str::<CaptainVerdict>(&verdict_text) {
        Ok(verdict) => Some(validate_verdict(verdict, item)),
        Err(e) => {
            warn!(module = "captain", %e,
                preview = &verdict_text[..verdict_text.floor_char_boundary(200)],
                "failed to parse captain review verdict, defaulting to escalate");
            Some(CaptainVerdict {
                action: "escalate".into(),
                feedback: format!("Failed to parse review verdict: {e}"),
                report: Some(format!(
                    "Captain review verdict could not be parsed as JSON. \
                     Raw text (first 200 chars): {}",
                    &verdict_text[..verdict_text.floor_char_boundary(200)]
                )),
            })
        }
    }
}

/// Validate a parsed verdict against the trigger's allowed actions.
fn validate_verdict(verdict: CaptainVerdict, item: &Task) -> CaptainVerdict {
    let trigger = item
        .captain_review_trigger
        .map(|t| t.as_str())
        .unwrap_or("unknown");
    if !is_verdict_allowed(trigger, &verdict.action) {
        warn!(module = "captain", action = %verdict.action, %trigger,
            "verdict not allowed for trigger, defaulting to escalate");
        CaptainVerdict {
            action: "escalate".into(),
            feedback: format!(
                "Invalid action '{}' for trigger '{trigger}'. {}",
                verdict.action, verdict.feedback
            ),
            report: Some(verdict.report.unwrap_or_else(|| {
                format!(
                    "Captain review returned invalid action '{}' for trigger '{trigger}'. \
                     Original feedback: {}",
                    verdict.action, verdict.feedback
                )
            })),
        }
    } else {
        verdict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verdict_schema_is_valid_json() {
        let schema = verdict_json_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["feedback"].is_object());
        assert!(schema["properties"]["report"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("action")));
        assert!(required.contains(&serde_json::json!("feedback")));
        // Action field must have enum constraint.
        let action_enum = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert!(action_enum.contains(&serde_json::json!("ship")));
        assert!(action_enum.contains(&serde_json::json!("nudge")));
        assert!(action_enum.contains(&serde_json::json!("escalate")));
        assert!(action_enum.contains(&serde_json::json!("respawn")));
        assert!(action_enum.contains(&serde_json::json!("retry_clarifier")));
    }

    #[test]
    fn test_template_renders_gates_pass_verdicts() {
        let workflow = mando_config::workflow::CaptainWorkflow::compiled_default();
        let mut vars = std::collections::HashMap::new();
        vars.insert("trigger", "gates_pass");
        vars.insert("title", "Test task");
        vars.insert("item_id", "42");
        vars.insert(
            "worker_contexts",
            "### Worker: test-worker\n- Status: in-progress",
        );
        vars.insert("knowledge_base", "");
        vars.insert("evidence_images", "");
        vars.insert("intervention_count", "3");
        vars.insert("is_gates_pass", "true");
        vars.insert("is_timeout", "");
        vars.insert("is_broken_session", "");
        vars.insert("is_budget_exhausted", "");
        vars.insert("is_clarifier_fail", "");
        vars.insert("is_rebase_fail", "");

        let rendered =
            mando_config::render_prompt("captain_review", &workflow.prompts, &vars).unwrap();

        // Worker context is populated.
        assert!(
            rendered.contains("test-worker"),
            "should contain worker context"
        );
        // Allowed verdicts for gates_pass are present.
        assert!(rendered.contains("**ship**"), "should have ship verdict");
        assert!(rendered.contains("**nudge**"), "should have nudge verdict");
        assert!(
            rendered.contains("**escalate**"),
            "should have escalate verdict"
        );
        // Other trigger verdicts should NOT be present.
        assert!(
            !rendered.contains("**respawn**"),
            "no respawn for gates_pass"
        );
        assert!(
            !rendered.contains("**retry_clarifier**"),
            "no retry_clarifier for gates_pass"
        );
    }

    #[test]
    fn test_template_renders_timeout_verdicts() {
        let workflow = mando_config::workflow::CaptainWorkflow::compiled_default();
        let mut vars = std::collections::HashMap::new();
        vars.insert("trigger", "timeout");
        vars.insert("title", "Test");
        vars.insert("item_id", "1");
        vars.insert("worker_contexts", "");
        vars.insert("knowledge_base", "");
        vars.insert("evidence_images", "");
        vars.insert("intervention_count", "0");
        vars.insert("is_gates_pass", "");
        vars.insert("is_timeout", "true");
        vars.insert("is_broken_session", "");
        vars.insert("is_budget_exhausted", "");
        vars.insert("is_clarifier_fail", "");
        vars.insert("is_rebase_fail", "");

        let rendered =
            mando_config::render_prompt("captain_review", &workflow.prompts, &vars).unwrap();

        assert!(rendered.contains("**nudge**"), "timeout should have nudge");
        assert!(
            rendered.contains("**escalate**"),
            "timeout should have escalate"
        );
        assert!(
            !rendered.contains("**ship**"),
            "timeout should not have ship"
        );
    }

    #[test]
    fn test_check_review_parses_structured_output() {
        use std::io::Write;

        let session_id = "test-check-review-structured";
        let stream_path = mando_config::stream_path_for_session(session_id);
        std::fs::create_dir_all(stream_path.parent().unwrap()).unwrap();

        let mut f = std::fs::File::create(&stream_path).unwrap();
        writeln!(
            f,
            r#"{{"type":"system","subtype":"init","session_id":"{session_id}"}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"result","subtype":"success","result":"","structured_output":{{"action":"ship","feedback":"looks good"}}}}"#
        )
        .unwrap();

        let item = Task {
            session_ids: mando_types::SessionIds {
                review: Some(session_id.to_string()),
                ..Default::default()
            },
            captain_review_trigger: Some(mando_types::task::ReviewTrigger::GatesPass),
            ..Task::new("test")
        };

        let verdict = check_review(&item).unwrap();
        assert_eq!(verdict.action, "ship");
        assert_eq!(verdict.feedback, "looks good");

        std::fs::remove_file(&stream_path).ok();
    }

    #[test]
    fn test_check_review_falls_back_to_assistant_text() {
        use std::io::Write;

        let session_id = "test-check-review-fallback";
        let stream_path = mando_config::stream_path_for_session(session_id);
        std::fs::create_dir_all(stream_path.parent().unwrap()).unwrap();

        let mut f = std::fs::File::create(&stream_path).unwrap();
        writeln!(
            f,
            r#"{{"type":"system","subtype":"init","session_id":"{session_id}"}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{{\"action\":\"nudge\",\"feedback\":\"add tests\"}}"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"result","subtype":"success","result":"","structured_output":null}}"#
        )
        .unwrap();

        let item = Task {
            session_ids: mando_types::SessionIds {
                review: Some(session_id.to_string()),
                ..Default::default()
            },
            captain_review_trigger: Some(mando_types::task::ReviewTrigger::GatesPass),
            ..Task::new("test")
        };

        let verdict = check_review(&item).unwrap();
        assert_eq!(verdict.action, "nudge");
        assert_eq!(verdict.feedback, "add tests");

        std::fs::remove_file(&stream_path).ok();
    }

    #[test]
    fn test_check_review_escalates_when_all_paths_empty() {
        use std::io::Write;

        let session_id = "test-check-review-all-empty";
        let stream_path = mando_config::stream_path_for_session(session_id);
        std::fs::create_dir_all(stream_path.parent().unwrap()).unwrap();

        // Session completed but no structured_output, no result text, no assistant text.
        let mut f = std::fs::File::create(&stream_path).unwrap();
        writeln!(
            f,
            r#"{{"type":"system","subtype":"init","session_id":"{session_id}"}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"result","subtype":"success","result":"","structured_output":null}}"#
        )
        .unwrap();

        let item = Task {
            session_ids: mando_types::SessionIds {
                review: Some(session_id.to_string()),
                ..Default::default()
            },
            captain_review_trigger: Some(mando_types::task::ReviewTrigger::GatesPass),
            ..Task::new("test")
        };

        let verdict = check_review(&item).unwrap();
        assert_eq!(verdict.action, "escalate");
        assert!(verdict.feedback.contains("no extractable verdict"));
        assert!(
            verdict.report.is_some(),
            "escalation must have a CTO report"
        );

        std::fs::remove_file(&stream_path).ok();
    }

    #[test]
    fn test_validate_verdict_rejects_invalid_action() {
        let item = Task {
            captain_review_trigger: Some(mando_types::task::ReviewTrigger::GatesPass),
            ..Task::new("test")
        };
        let verdict = CaptainVerdict {
            action: "approve".into(),
            feedback: "looks good".into(),
            report: None,
        };
        let result = validate_verdict(verdict, &item);
        assert_eq!(result.action, "escalate");
        assert!(result.feedback.contains("approve"));
    }
}
