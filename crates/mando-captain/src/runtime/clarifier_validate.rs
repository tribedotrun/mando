//! Clarifier output validation — schema construction and retry on invalid output.

use anyhow::Result;
use mando_config::workflow::CaptainWorkflow;
use tracing::{info, warn};

use mando_cc::{CcConfig, CcOneShot};

use super::clarifier::{parse_clarifier_response, ClarifierResult};

/// Build the JSON schema for clarifier output, with `repo` constrained to
/// an enum of valid project names.
pub(crate) fn build_clarifier_schema(valid_names: &[String]) -> serde_json::Value {
    let mut repo_enum: Vec<serde_json::Value> =
        valid_names.iter().map(|n| serde_json::json!(n)).collect();
    repo_enum.push(serde_json::Value::Null);

    serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string", "enum": ["ready", "clarifying", "escalate", "answered"] },
            "context": { "type": "string" },
            "questions": {
                "type": ["array", "null"],
                "items": {
                    "type": "object",
                    "properties": {
                        "question": { "type": "string" },
                        "answer": { "type": ["string", "null"] },
                        "self_answered": { "type": "boolean" },
                        "category": { "type": "string", "enum": ["code", "intent"] }
                    },
                    "required": ["question", "self_answered", "category"]
                }
            },
            "title": { "type": ["string", "null"] },
            "repo": { "enum": repo_enum },
            "no_pr": { "type": ["boolean", "null"] },
            "resource": { "type": ["string", "null"] }
        },
        "required": ["status", "context"]
    })
}

/// Check if the clarifier's `repo` field is valid. Returns the invalid value
/// if validation fails, or `None` if valid (or repo is absent/empty).
pub(crate) fn check_repo(parsed: &ClarifierResult, valid_names: &[String]) -> Option<String> {
    let repo = parsed.repo.as_ref()?;
    if repo.trim().is_empty() {
        return None;
    }
    if valid_names.iter().any(|n| n == repo) {
        return None;
    }
    Some(repo.clone())
}

/// Resume the clarifier session and ask it to fix the invalid `repo` field.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn retry_with_correction(
    session_id: &str,
    bad_repo: &str,
    valid_names: &[String],
    schema: &serde_json::Value,
    cwd: &std::path::Path,
    workflow: &CaptainWorkflow,
    task_id: &str,
    item_title: &str,
    pool: &sqlx::SqlitePool,
) -> Result<ClarifierResult> {
    let names_list = valid_names.join(", ");
    let correction_prompt = format!(
        "Your previous output had an invalid `repo` field: \"{bad_repo}\". \
         The `repo` field must be one of: [{names_list}] or null. \
         Please provide your complete output again with a valid `repo` value."
    );

    let result = CcOneShot::run(
        &correction_prompt,
        CcConfig::builder()
            .model(&workflow.models.clarifier)
            .timeout(Duration::from_secs(workflow.agent.clarifier_timeout_s))
            .caller("clarifier-retry")
            .task_id(task_id)
            .cwd(cwd)
            .resume(session_id)
            .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
            .json_schema(schema.clone())
            .build(),
    )
    .await?;

    crate::io::headless_cc::log_cc_session(
        pool,
        &crate::io::headless_cc::SessionLogEntry {
            session_id: &result.session_id,
            cwd,
            model: &workflow.models.clarifier,
            caller: "clarifier-retry",
            cost_usd: result.cost_usd,
            duration_ms: result.duration_ms,
            resumed: true,
            task_id,
            status: mando_types::SessionStatus::Stopped,
            worker_name: "",
        },
    )
    .await;

    let text = result
        .structured
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| result.text.clone());
    let mut parsed = parse_clarifier_response(&text, item_title);
    parsed.session_id = Some(result.session_id);

    // If still invalid after retry, clear the repo rather than storing garbage.
    if check_repo(&parsed, valid_names).is_some() {
        warn!(
            module = "clarifier",
            repo = ?parsed.repo,
            "clarifier returned invalid project name again after retry — clearing"
        );
        parsed.repo = None;
    }

    info!(
        module = "clarifier",
        title = %&item_title[..item_title.len().min(60)],
        status = ?parsed.status,
        "clarification complete (after retry)"
    );
    Ok(parsed)
}

use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::clarifier::ClarifierStatus;

    #[test]
    fn schema_constrains_repo_to_project_names() {
        let names = vec!["mando".into(), "acme-web".into()];
        let schema = build_clarifier_schema(&names);

        let repo_schema = &schema["properties"]["repo"];
        let enum_values = repo_schema["enum"].as_array().unwrap();
        assert_eq!(enum_values.len(), 3); // mando, acme-web, null
        assert!(enum_values.contains(&serde_json::json!("mando")));
        assert!(enum_values.contains(&serde_json::json!("acme-web")));
        assert!(enum_values.contains(&serde_json::Value::Null));
        // Must NOT contain a free-form type — enum is the sole constraint.
        assert!(repo_schema.get("type").is_none());
    }

    #[test]
    fn schema_handles_empty_projects() {
        let names: Vec<String> = vec![];
        let schema = build_clarifier_schema(&names);

        let enum_values = schema["properties"]["repo"]["enum"].as_array().unwrap();
        assert_eq!(enum_values.len(), 1); // only null
        assert_eq!(enum_values[0], serde_json::Value::Null);
    }

    #[test]
    fn schema_includes_category_field() {
        let names = vec!["mando".into()];
        let schema = build_clarifier_schema(&names);
        let items = &schema["properties"]["questions"]["items"];
        let props = &items["properties"];
        assert!(props.get("category").is_some());
        let cat_enum = props["category"]["enum"].as_array().unwrap();
        assert!(cat_enum.contains(&serde_json::json!("code")));
        assert!(cat_enum.contains(&serde_json::json!("intent")));
        let required = items["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("category")));
    }

    #[test]
    fn check_repo_valid_name() {
        let names = vec!["mando".into(), "acme".into()];
        let mut result = ClarifierResult {
            status: ClarifierStatus::Ready,
            context: "ctx".into(),
            questions: None,
            generated_title: None,
            repo: Some("mando".into()),
            no_pr: None,
            resource: None,
            session_id: None,
        };
        assert!(check_repo(&result, &names).is_none());

        result.repo = Some("acme/widgets".into());
        assert_eq!(check_repo(&result, &names).as_deref(), Some("acme/widgets"));

        result.repo = None;
        assert!(check_repo(&result, &names).is_none());

        result.repo = Some("".into());
        assert!(check_repo(&result, &names).is_none());
    }
}
