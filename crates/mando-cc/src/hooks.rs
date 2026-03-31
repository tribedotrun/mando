//! Hook definitions for the control protocol.
//!
//! Hooks run in the daemon process, NOT in Claude's context window.
//! They intercept tool calls and agent lifecycle events via the
//! bidirectional stream-json control protocol.

use serde_json::Value;

/// A hook that can be registered with a CC session.
#[derive(Clone)]
pub enum Hook {}

/// Dispatch a control request through registered hooks.
///
/// Returns the response JSON to send back via stdin.
pub fn dispatch_hook(_hooks: &[Hook], subtype: &str, request_id: &str, _payload: &Value) -> Value {
    match subtype {
        "can_use_tool" => {
            // No hook variants exist — always allow.
            crate::protocol::control_response_allow(request_id)
        }
        "hook_callback" => {
            // No observer hooks — return empty success.
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
    use serde_json::Value;

    /// Decision from a PreToolUse hook (test-only).
    enum PreToolUseDecision {
        Allow,
        #[allow(dead_code)]
        AllowWithInput(Value),
        Deny(String),
    }

    /// PreToolUse hook (test-only).
    struct PreToolUseHook {
        matcher: Option<String>,
        evaluate: fn(&str, &Value) -> PreToolUseDecision,
    }

    /// PreToolUse hook: block dangerous bash commands (test-only).
    fn safety_bash_guardrail() -> PreToolUseHook {
        PreToolUseHook {
            matcher: Some("Bash".into()),
            evaluate: |_tool_name, input| {
                let command = input.get("command").and_then(|c| c.as_str()).unwrap_or("");

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
}
