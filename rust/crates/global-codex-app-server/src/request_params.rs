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
