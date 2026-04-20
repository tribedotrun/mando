use super::*;

#[test]
fn test_verdict_schema_is_trigger_aware() {
    // Default triggers: ship, nudge, respawn, reset_budget (no escalate).
    let schema = verdict_json_schema("gates_pass");
    assert_eq!(schema["type"], "object");
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("action")));
    assert!(required.contains(&serde_json::json!("feedback")));
    let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(actions.contains(&serde_json::json!("ship")));
    assert!(actions.contains(&serde_json::json!("nudge")));
    assert!(actions.contains(&serde_json::json!("respawn")));
    assert!(actions.contains(&serde_json::json!("reset_budget")));
    assert!(!actions.contains(&serde_json::json!("escalate")));
    assert!(!actions.contains(&serde_json::json!("retry_clarifier")));

    // budget_exhausted: includes escalate.
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
