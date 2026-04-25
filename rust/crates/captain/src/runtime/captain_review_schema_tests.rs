use super::*;

#[test]
fn test_verdict_schema_is_trigger_aware() {
    // Default triggers (gates_pass, repeated_nudge, timeout,
    // degraded_context, rebase_fail, ci_failure, merge_fail): ship, nudge,
    // respawn, reset_budget, and escalate.
    for trigger in [
        "gates_pass",
        "repeated_nudge",
        "timeout",
        "degraded_context",
        "rebase_fail",
        "ci_failure",
        "merge_fail",
    ] {
        let schema = verdict_json_schema(trigger);
        assert_eq!(schema["type"], "object", "{trigger}");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("action")), "{trigger}");
        assert!(
            required.contains(&serde_json::json!("feedback")),
            "{trigger}"
        );
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert!(actions.contains(&serde_json::json!("ship")), "{trigger}");
        assert!(actions.contains(&serde_json::json!("nudge")), "{trigger}");
        assert!(actions.contains(&serde_json::json!("respawn")), "{trigger}");
        assert!(
            actions.contains(&serde_json::json!("reset_budget")),
            "{trigger}"
        );
        assert!(
            actions.contains(&serde_json::json!("escalate")),
            "{trigger}"
        );
        assert!(
            !actions.contains(&serde_json::json!("retry_clarifier")),
            "{trigger}"
        );
    }

    // broken_session: only ship, respawn, or escalate. A broken session
    // should never resume in place via nudge/reset_budget.
    let schema = verdict_json_schema("broken_session");
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(actions.contains(&serde_json::json!("ship")));
    assert!(actions.contains(&serde_json::json!("respawn")));
    assert!(actions.contains(&serde_json::json!("escalate")));
    assert!(!actions.contains(&serde_json::json!("nudge")));
    assert!(!actions.contains(&serde_json::json!("reset_budget")));
    assert!(!actions.contains(&serde_json::json!("retry_clarifier")));

    // budget_exhausted routes through the default tier now and still
    // includes escalate + reset_budget.
    let schema = verdict_json_schema("budget_exhausted");
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(actions.contains(&serde_json::json!("escalate")));
    assert!(actions.contains(&serde_json::json!("reset_budget")));
    assert!(!actions.contains(&serde_json::json!("retry_clarifier")));

    // clarifier_fail: only retry_clarifier and escalate.
    let schema = verdict_json_schema("clarifier_fail");
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(actions.contains(&serde_json::json!("retry_clarifier")));
    assert!(actions.contains(&serde_json::json!("escalate")));
    assert!(!actions.contains(&serde_json::json!("ship")));

    // spawn_fail: only respawn and escalate.
    let schema = verdict_json_schema("spawn_fail");
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(actions.contains(&serde_json::json!("respawn")));
    assert!(actions.contains(&serde_json::json!("escalate")));
    assert!(!actions.contains(&serde_json::json!("ship")));
    assert!(!actions.contains(&serde_json::json!("retry_clarifier")));
}
