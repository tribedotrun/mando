use serde_json::{json, Map, Value};

use crate::types::StartTurnRequest;

const COMPUTER_USE_MCP_SERVER: &str = "computer-use";
const COMPUTER_USE_PLUGIN: &str = "computer-use@openai-bundled";

pub(crate) fn thread_params(request: &StartTurnRequest) -> Value {
    let mut params = Map::new();
    if let Some(thread_id) = &request.resume_thread_id {
        params.insert("threadId".into(), json!(thread_id));
    }
    params.insert("cwd".into(), json!(request.cwd.display().to_string()));
    params.insert("approvalPolicy".into(), json!(request.approval_policy));
    params.insert(
        "approvalsReviewer".into(),
        json!(request.approvals_reviewer),
    );
    params.insert("sandbox".into(), json!(request.sandbox));
    params.insert(
        "model".into(),
        request
            .codex
            .model
            .as_ref()
            .map_or(Value::Null, |model| json!(model)),
    );
    params.insert("modelProvider".into(), Value::Null);
    if let Some(service_tier) = &request.codex.service_tier {
        params.insert("serviceTier".into(), json!(service_tier));
    }
    if request.resume_thread_id.is_none() {
        params.insert("serviceName".into(), json!("mando"));
        params.insert("threadSource".into(), json!("subagent"));
    }
    Value::Object(params)
}

pub(crate) fn turn_params(thread_id: &str, request: &StartTurnRequest) -> Value {
    let mut params = Map::new();
    params.insert("threadId".into(), json!(thread_id));
    params.insert(
        "input".into(),
        json!([{"type": "text", "text": request.prompt}]),
    );
    params.insert("approvalPolicy".into(), json!(request.approval_policy));
    params.insert(
        "approvalsReviewer".into(),
        json!(request.approvals_reviewer),
    );
    params.insert("sandboxPolicy".into(), request.sandbox_policy.clone());
    params.insert("cwd".into(), json!(request.cwd.display().to_string()));
    params.insert(
        "model".into(),
        request
            .codex
            .model
            .as_ref()
            .map_or(Value::Null, |model| json!(model)),
    );
    if let Some(reasoning_effort) = &request.codex.reasoning_effort {
        params.insert("effort".into(), json!(reasoning_effort));
    }
    if let Some(service_tier) = &request.codex.service_tier {
        params.insert("serviceTier".into(), json!(service_tier));
    }
    if let Some(schema) = &request.output_schema {
        params.insert("outputSchema".into(), schema.clone());
    }
    Value::Object(params)
}

pub(crate) fn computer_use_mcp_approval_params() -> Value {
    json!({
        "keyPath": format!("plugins.\"{COMPUTER_USE_PLUGIN}\".mcp_servers.{COMPUTER_USE_MCP_SERVER}.default_tools_approval_mode"),
        "value": "approve",
        "mergeStrategy": "replace",
        "filePath": Value::Null,
        "expectedVersion": Value::Null,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::types::CodexTurnConfig;

    fn request(codex: CodexTurnConfig) -> StartTurnRequest {
        StartTurnRequest {
            cwd: PathBuf::from("/tmp/work"),
            prompt: "do it".into(),
            resume_thread_id: None,
            output_schema: None,
            codex,
            sandbox: "danger-full-access".into(),
            sandbox_policy: json!({"type": "dangerFullAccess"}),
            approval_policy: "on-request".into(),
            approvals_reviewer: "auto_review".into(),
            response_timeout: Duration::from_secs(30),
        }
    }

    #[test]
    fn params_include_configured_model_effort_and_standard_tier() {
        let req = request(CodexTurnConfig {
            model: Some("gpt-5.4".into()),
            reasoning_effort: Some("medium".into()),
            service_tier: Some("default".into()),
        });

        let thread = thread_params(&req);
        let turn = turn_params("thread-1", &req);

        assert_eq!(thread.get("model"), Some(&json!("gpt-5.4")));
        assert_eq!(thread.get("serviceTier"), Some(&json!("default")));
        assert_eq!(turn.get("model"), Some(&json!("gpt-5.4")));
        assert_eq!(turn.get("effort"), Some(&json!("medium")));
        assert_eq!(turn.get("serviceTier"), Some(&json!("default")));
    }

    #[test]
    fn params_preserve_null_model_when_not_overridden() {
        let req = request(CodexTurnConfig::default());

        let thread = thread_params(&req);
        let turn = turn_params("thread-1", &req);

        assert_eq!(thread.get("model"), Some(&Value::Null));
        assert_eq!(turn.get("model"), Some(&Value::Null));
        assert_eq!(turn.get("effort"), None);
        assert_eq!(turn.get("serviceTier"), None);
    }

    #[test]
    fn params_use_configured_sandbox_for_thread_and_turn() {
        let req = request(CodexTurnConfig::default());

        let thread = thread_params(&req);
        let turn = turn_params("thread-1", &req);

        assert_eq!(thread.get("sandbox"), Some(&json!("danger-full-access")));
        assert_eq!(
            turn.get("sandboxPolicy"),
            Some(&json!({ "type": "dangerFullAccess" }))
        );
    }

    #[test]
    fn turn_params_include_output_schema_only_when_requested() {
        let mut text_req = request(CodexTurnConfig::default());
        text_req.output_schema = None;
        let text_turn = turn_params("thread-1", &text_req);
        assert_eq!(text_turn.get("outputSchema"), None);

        let mut structured_req = request(CodexTurnConfig::default());
        structured_req.output_schema = Some(json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"],
            "additionalProperties": false
        }));
        let structured_turn = turn_params("thread-1", &structured_req);
        assert_eq!(
            structured_turn.get("outputSchema"),
            structured_req.output_schema.as_ref()
        );
    }

    #[test]
    fn computer_use_mcp_approval_params_force_approve_mode() {
        assert_eq!(
            computer_use_mcp_approval_params(),
            json!({
                "keyPath": "plugins.\"computer-use@openai-bundled\".mcp_servers.computer-use.default_tools_approval_mode",
                "value": "approve",
                "mergeStrategy": "replace",
                "filePath": null,
                "expectedVersion": null,
            })
        );
    }
}
