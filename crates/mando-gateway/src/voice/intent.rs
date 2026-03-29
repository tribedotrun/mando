//! Voice intent — calls headless Claude Code with full API access to answer user queries.
//!
//! Claude gets the daemon's base URL + auth token and can call any endpoint autonomously.
//! Returns a spoken response directly — no action parsing or executor needed.

use std::collections::HashMap;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use mando_cc::{CcConfig, CcOneShot};
use sqlx::SqlitePool;

/// Response from the voice agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VoiceResponse {
    pub response: String,
}

/// Run the voice agent — Claude with full daemon API access.
/// Returns a spoken response ready for TTS.
pub(crate) async fn run_voice_agent(
    message: &str,
    conversation_history: &str,
    prompt_template: &str,
    daemon_url: &str,
    auth_token: &str,
    pool: &SqlitePool,
) -> Result<String> {
    let mut vars = HashMap::new();
    vars.insert("transcript", message);
    vars.insert("conversation_history", conversation_history);
    vars.insert("daemon_url", daemon_url);
    vars.insert("auth_token", auth_token);

    let prompt = mando_config::render_template(prompt_template, &vars)
        .map_err(|e| anyhow::anyhow!("failed to render voice prompt: {e}"))?;

    let result = CcOneShot::run(
        &prompt,
        CcConfig::builder()
            .model("sonnet")
            .timeout(std::time::Duration::from_secs(120))
            .caller("voice-agent")
            .json_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "response": { "type": "string" }
                },
                "required": ["response"]
            }))
            .build(),
    )
    .await?;

    mando_captain::io::headless_cc::log_cc_session(
        pool,
        &mando_captain::io::headless_cc::SessionLogEntry {
            session_id: &result.session_id,
            cwd: std::path::Path::new(""),
            model: "sonnet",
            caller: "voice-agent",
            cost_usd: result.cost_usd,
            duration_ms: result.duration_ms,
            resumed: false,
            task_id: "",
            status: mando_types::SessionStatus::Stopped,
            worker_name: "",
        },
    )
    .await;

    if let Some(structured) = result.structured {
        if let Some(resp) = structured.get("response").and_then(|v| v.as_str()) {
            return Ok(resp.to_string());
        }
    }
    parse_response(&result.text)
}

/// Extract the spoken response from Claude's output.
fn parse_response(raw: &str) -> Result<String> {
    let trimmed = raw.trim();

    // Try direct JSON parse.
    if let Ok(resp) = serde_json::from_str::<VoiceResponse>(trimmed) {
        return Ok(resp.response);
    }

    // Try extracting from markdown code block.
    if let Some(json_str) = extract_json_from_markdown(trimmed) {
        if let Ok(resp) = serde_json::from_str::<VoiceResponse>(&json_str) {
            return Ok(resp.response);
        }
    }

    // Try finding first { ... } block.
    if let Some(json_str) = extract_first_json_object(trimmed) {
        if let Ok(resp) = serde_json::from_str::<VoiceResponse>(&json_str) {
            return Ok(resp.response);
        }
    }

    // Fallback: if Claude just returned plain text (no JSON), use it directly.
    // This handles cases where Claude ignores the JSON format instruction.
    if !trimmed.is_empty() && !trimmed.starts_with('{') {
        return Ok(trimmed.to_string());
    }

    let preview: String = trimmed.chars().take(200).collect();
    bail!("failed to parse voice response: {preview}")
}

fn extract_json_from_markdown(s: &str) -> Option<String> {
    let start = s.find("```")?;
    let after = &s[start + 3..];
    let content_start = after.find('\n')? + 1;
    let content = &after[content_start..];
    let end = content.find("```")?;
    Some(content[..end].trim().to_string())
}

fn extract_first_json_object(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in s[start..].char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..start + i + 1].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clean_json() {
        let raw = r#"{"response":"You have three items in your task list."}"#;
        let resp = parse_response(raw).unwrap();
        assert_eq!(resp, "You have three items in your task list.");
    }

    #[test]
    fn parse_json_in_code_block() {
        let raw = "```json\n{\"response\":\"All good.\"}\n```";
        let resp = parse_response(raw).unwrap();
        assert_eq!(resp, "All good.");
    }

    #[test]
    fn parse_plain_text_fallback() {
        let raw = "You have three items in your task list, two are stuck.";
        let resp = parse_response(raw).unwrap();
        assert_eq!(resp, raw);
    }

    #[test]
    fn parse_invalid_fails() {
        let raw = "{}";
        let result = parse_response(raw);
        assert!(result.is_err());
    }
}
