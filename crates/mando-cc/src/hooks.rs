//! Hook definitions for the control protocol.
//!
//! Hooks run in the daemon process, NOT in Claude's context window.
//! They intercept tool calls and agent lifecycle events via the
//! bidirectional stream-json control protocol.

use serde_json::Value;

/// A hook that can be registered with a CC session.
#[derive(Clone)]
pub enum Hook {
    /// Fires before a tool is executed. Can allow, deny, or modify input.
    PreToolUse(PreToolUseHook),
    /// Fires after a tool executes. Can record metrics.
    PostToolUse(PostToolUseHook),
    /// Fires when a subagent starts.
    SubagentStart(SubagentHook),
    /// Fires when a subagent stops.
    SubagentStop(SubagentHook),
    /// Fires when the session stops.
    Stop(StopHook),
}

/// Decision from a PreToolUse hook.
pub enum PreToolUseDecision {
    /// Allow the tool call.
    Allow,
    /// Allow with modified input.
    AllowWithInput(Value),
    /// Deny with a reason message.
    Deny(String),
}

/// PreToolUse hook — evaluates before each tool execution.
#[derive(Clone)]
pub struct PreToolUseHook {
    /// Tool name pattern to match (None = all tools).
    pub matcher: Option<String>,
    /// The evaluation function. Takes (tool_name, tool_input) → decision.
    pub evaluate: fn(&str, &Value) -> PreToolUseDecision,
}

/// PostToolUse hook — fires after each tool execution for metrics.
#[derive(Clone)]
pub struct PostToolUseHook {
    /// Tool name pattern to match (None = all tools).
    pub matcher: Option<String>,
    /// The callback. Takes (tool_name, tool_input, tool_response).
    pub callback: fn(&str, &Value, &Value),
}

/// Subagent lifecycle hook.
#[derive(Clone)]
pub struct SubagentHook {
    /// Callback. Takes (agent_id, agent_type).
    pub callback: fn(&str, &str),
}

/// Stop hook — fires at session end.
#[derive(Clone)]
pub struct StopHook {
    /// Callback. Takes (session_id, stop_reason).
    pub callback: fn(&str, &str),
}

// ── Built-in safety hooks ────────────────────────────────────────────────────

/// PreToolUse hook: block dangerous bash commands.
pub fn safety_bash_guardrail() -> PreToolUseHook {
    PreToolUseHook {
        matcher: Some("Bash".into()),
        evaluate: |_tool_name, input| {
            let command = input.get("command").and_then(|c| c.as_str()).unwrap_or("");

            // Block rm -rf on non-tmp paths.
            if command.contains("rm -rf")
                && !command.contains("/tmp/")
                && !command.contains("node_modules")
                && !command.contains("target/")
            {
                return PreToolUseDecision::Deny(
                    "Blocked: rm -rf outside tmp/node_modules/target".into(),
                );
            }

            PreToolUseDecision::Allow
        },
    }
}

/// Dispatch a control request through registered hooks.
///
/// Returns the response JSON to send back via stdin.
pub fn dispatch_hook(hooks: &[Hook], subtype: &str, request_id: &str, payload: &Value) -> Value {
    match subtype {
        "can_use_tool" => {
            let tool_name = payload
                .get("tool_name")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let input = payload.get("input").cloned().unwrap_or(Value::Null);

            for hook in hooks {
                if let Hook::PreToolUse(h) = hook {
                    // Check matcher.
                    if let Some(ref matcher) = h.matcher {
                        if matcher != tool_name {
                            continue;
                        }
                    }
                    match (h.evaluate)(tool_name, &input) {
                        PreToolUseDecision::Allow => {}
                        PreToolUseDecision::AllowWithInput(new_input) => {
                            return crate::protocol::control_response_allow_with_input(
                                request_id, &new_input,
                            );
                        }
                        PreToolUseDecision::Deny(msg) => {
                            return crate::protocol::control_response_deny(request_id, &msg);
                        }
                    }
                }
            }

            // No hook denied — allow.
            crate::protocol::control_response_allow(request_id)
        }
        "hook_callback" => {
            // Hook callback from CLI — check event type.
            let hook_event = payload
                .get("input")
                .and_then(|i| i.get("hook_event_name"))
                .and_then(|e| e.as_str())
                .unwrap_or("");

            match hook_event {
                "PostToolUse" => {
                    let tool_name = payload
                        .get("input")
                        .and_then(|i| i.get("tool_name"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    let tool_input = payload
                        .get("input")
                        .and_then(|i| i.get("tool_input"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    let tool_response = payload
                        .get("input")
                        .and_then(|i| i.get("tool_response"))
                        .cloned()
                        .unwrap_or(Value::Null);

                    for hook in hooks {
                        if let Hook::PostToolUse(h) = hook {
                            if let Some(ref matcher) = h.matcher {
                                if matcher != tool_name {
                                    continue;
                                }
                            }
                            (h.callback)(tool_name, &tool_input, &tool_response);
                        }
                    }
                }
                "SubagentStart" => {
                    let agent_id = payload
                        .get("input")
                        .and_then(|i| i.get("agent_id"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("");
                    let agent_type = payload
                        .get("input")
                        .and_then(|i| i.get("agent_type"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("");
                    for hook in hooks {
                        if let Hook::SubagentStart(h) = hook {
                            (h.callback)(agent_id, agent_type);
                        }
                    }
                }
                "SubagentStop" => {
                    let agent_id = payload
                        .get("input")
                        .and_then(|i| i.get("agent_id"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("");
                    let agent_type = payload
                        .get("input")
                        .and_then(|i| i.get("agent_type"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("");
                    for hook in hooks {
                        if let Hook::SubagentStop(h) = hook {
                            (h.callback)(agent_id, agent_type);
                        }
                    }
                }
                "Stop" => {
                    let session_id = payload
                        .get("input")
                        .and_then(|i| i.get("session_id"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("");
                    let stop_reason = payload
                        .get("input")
                        .and_then(|i| i.get("stop_reason"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("");
                    for hook in hooks {
                        if let Hook::Stop(h) = hook {
                            (h.callback)(session_id, stop_reason);
                        }
                    }
                }
                _ => {}
            }

            // Hooks that observe (PostToolUse, Subagent*, Stop) always return empty success.
            serde_json::json!({
                "type": "control_response",
                "response": {
                    "subtype": "success",
                    "request_id": request_id,
                    "response": {}
                }
            })
        }
        _ => crate::protocol::control_response_init(request_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_hook_allows_normal_commands() {
        let hook = safety_bash_guardrail();
        let input = serde_json::json!({"command": "cargo nextest run -p mando-types"});
        assert!(matches!(
            (hook.evaluate)("Bash", &input),
            PreToolUseDecision::Allow
        ));
    }

    #[test]
    fn safety_hook_blocks_rm_rf() {
        let hook = safety_bash_guardrail();
        let input = serde_json::json!({"command": "rm -rf /home/user/project"});
        assert!(matches!(
            (hook.evaluate)("Bash", &input),
            PreToolUseDecision::Deny(_)
        ));
    }

    #[test]
    fn safety_hook_allows_rm_rf_in_tmp() {
        let hook = safety_bash_guardrail();
        let input = serde_json::json!({"command": "rm -rf /tmp/test"});
        assert!(matches!(
            (hook.evaluate)("Bash", &input),
            PreToolUseDecision::Allow
        ));
    }

    #[test]
    fn dispatch_allows_by_default() {
        let hooks = vec![];
        let payload = serde_json::json!({
            "tool_name": "Read",
            "input": {"path": "/tmp/foo"}
        });
        let response = dispatch_hook(&hooks, "can_use_tool", "req_1", &payload);
        assert_eq!(response["response"]["response"]["behavior"], "allow");
    }

    #[test]
    fn dispatch_denies_with_hook() {
        let hooks = vec![Hook::PreToolUse(safety_bash_guardrail())];
        let payload = serde_json::json!({
            "tool_name": "Bash",
            "input": {"command": "rm -rf /home/user/project"}
        });
        let response = dispatch_hook(&hooks, "can_use_tool", "req_1", &payload);
        assert_eq!(response["response"]["response"]["behavior"], "deny");
    }
}
