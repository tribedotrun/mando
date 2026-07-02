use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::io::{BufReader, Lines};
use tokio::process::ChildStdout;

#[derive(Default)]
pub struct OpenCodeStreamState {
    result_text: String,
    cost_usd: Option<f64>,
    usage: Option<Value>,
    error_text: Option<String>,
}

pub struct OpenCodeEvent(Value);

impl OpenCodeEvent {
    pub fn parse(line: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(line).context("parse opencode json event")?;
        Ok(Self(value))
    }

    fn session_id(&self) -> Option<String> {
        self.0
            .get("sessionID")
            .or_else(|| self.0.get("session_id"))
            .and_then(Value::as_str)
            .map(str::to_string)
    }
}

pub async fn read_until_session_id(
    lines: &mut Lines<BufReader<ChildStdout>>,
    fallback_session_id: Option<&str>,
) -> Result<(String, Vec<OpenCodeEvent>)> {
    let mut buffered_events = Vec::new();
    while let Some(line) = lines.next_line().await.context("read opencode stdout")? {
        if line.trim().is_empty() {
            continue;
        }
        let event = OpenCodeEvent::parse(&line)?;
        let session_id = event.session_id();
        buffered_events.push(event);
        if let Some(session_id) = session_id.or_else(|| fallback_session_id.map(str::to_string)) {
            return Ok((session_id, buffered_events));
        }
    }
    if let Some(session_id) = fallback_session_id {
        return Ok((session_id.to_string(), buffered_events));
    }
    anyhow::bail!("OpenCode exited before emitting a session id")
}

pub fn initial_stream_lines(session_id: &str, cwd: &Path, prompt: &str) -> Vec<Value> {
    vec![
        json!({
            "type": "system",
            "subtype": "init",
            "session_id": session_id,
            "provider": "opencode",
            "cwd": cwd.display().to_string(),
        }),
        json!({
            "type": "user",
            "message": {"content": [{"type": "text", "text": prompt}]},
        }),
    ]
}

pub fn normalize_event_lines(event: &OpenCodeEvent, state: &mut OpenCodeStreamState) -> Vec<Value> {
    let value = &event.0;
    match value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "text" => normalize_text_event(value, state),
        "tool_use" | "tool" => normalize_tool_event(value),
        "tool_result" => vec![
            json!({"type": "tool_result", "content": opencode_text(value).unwrap_or_default()}),
        ],
        "step_finish" => {
            let part = value.get("part").unwrap_or(value);
            state.cost_usd =
                add_optional_f64(state.cost_usd, part.get("cost").and_then(Value::as_f64));
            state.usage = Some(accumulate_usage(
                state.usage.take(),
                part.get("tokens").unwrap_or(&Value::Null),
            ));
            Vec::new()
        }
        "error" => normalize_error_event(value, state),
        _ => Vec::new(),
    }
}

fn normalize_text_event(value: &Value, state: &mut OpenCodeStreamState) -> Vec<Value> {
    match opencode_text(value) {
        Some(text) => {
            state.result_text.push_str(text);
            vec![json!({
                "type": "assistant",
                "message": {"content": [{"type": "text", "text": text}]},
            })]
        }
        None => Vec::new(),
    }
}

fn normalize_tool_event(value: &Value) -> Vec<Value> {
    let part = value.get("part").unwrap_or(value);
    let name = value
        .pointer("/part/tool")
        .or_else(|| value.pointer("/part/name"))
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("opencode_tool");
    let call_id = part
        .get("callID")
        .or_else(|| part.get("callId"))
        .or_else(|| part.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let input = part
        .pointer("/state/input")
        .or_else(|| part.get("input"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut content = json!({"type": "tool_use", "name": name});
    if !call_id.is_empty() {
        content["id"] = json!(call_id);
    }
    if !input.is_null() {
        content["input"] = input;
    }
    let mut lines = vec![json!({
        "type": "assistant",
        "message": {"content": [content]},
    })];
    if let Some(output) = tool_output_text(part) {
        lines.push(json!({"type": "tool_result", "content": output}));
    }
    lines
}

fn normalize_error_event(value: &Value, state: &mut OpenCodeStreamState) -> Vec<Value> {
    let text = opencode_text(value)
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string());
    state.error_text = Some(text.clone());
    vec![json!({"type": "tool_result", "content": text})]
}

fn tool_output_text(part: &Value) -> Option<String> {
    part.pointer("/state/output")
        .or_else(|| part.get("output"))
        .or_else(|| part.pointer("/state/error"))
        .or_else(|| part.get("error"))
        .map(value_to_text)
        .filter(|text| !text.is_empty())
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.to_string(),
        Value::Null => String::new(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

pub struct OpenCodeCompletion {
    pub final_status: global_types::SessionStatus,
    pub cost_usd: Option<f64>,
    pub result_line: Value,
}

pub fn result_stream_line(
    state: OpenCodeStreamState,
    status: std::io::Result<std::process::ExitStatus>,
    stdout_error: Option<String>,
    stderr_text: &str,
    duration_ms: i64,
    expected_termination: bool,
) -> OpenCodeCompletion {
    let process_success = status.as_ref().is_ok_and(std::process::ExitStatus::success);
    let error_text = if expected_termination {
        None
    } else {
        stdout_error.or(state.error_text).or_else(|| {
            (!process_success && !stderr_text.trim().is_empty())
                .then(|| stderr_text.trim().to_string())
        })
    };
    let final_status = if expected_termination || (process_success && error_text.is_none()) {
        global_types::SessionStatus::Stopped
    } else {
        global_types::SessionStatus::Failed
    };
    let interrupted_text = "OpenCode session stopped before completion";
    let result_text = if expected_termination && state.result_text.is_empty() {
        interrupted_text
    } else if state.result_text.is_empty() {
        error_text.as_deref().unwrap_or_default()
    } else {
        state.result_text.as_str()
    };
    let is_error = !expected_termination && final_status == global_types::SessionStatus::Failed;
    let subtype = if expected_termination {
        "interrupted"
    } else if is_error {
        "error"
    } else {
        "success"
    };
    OpenCodeCompletion {
        final_status,
        cost_usd: state.cost_usd,
        result_line: json!({
            "type": "result",
            "subtype": subtype,
            "is_error": is_error,
            "duration_ms": duration_ms,
            "num_turns": 1,
            "total_cost_usd": state.cost_usd,
            "usage": state.usage,
            "result": result_text,
            "structured_output": null,
            "structured_output_error": null,
            "error": error_text,
        }),
    }
}

fn opencode_text(value: &Value) -> Option<&str> {
    value
        .pointer("/part/text")
        .or_else(|| value.pointer("/part/content"))
        .or_else(|| value.get("text"))
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
}

fn normalize_usage(tokens: &Value) -> Value {
    json!({
        "input_tokens": token_count(tokens, &["input", "input_tokens", "prompt"]),
        "output_tokens": token_count(tokens, &["output", "output_tokens", "completion"]),
        "cache_read_input_tokens": token_count(tokens, &["cache_read", "cache_read_input_tokens"]),
        "cache_creation_input_tokens": token_count(tokens, &["cache_creation", "cache_creation_input_tokens"]),
    })
}

fn accumulate_usage(current: Option<Value>, tokens: &Value) -> Value {
    let normalized = normalize_usage(tokens);
    json!({
        "input_tokens": token_count_option(current.as_ref(), &["input_tokens"]) + token_count(&normalized, &["input_tokens"]),
        "output_tokens": token_count_option(current.as_ref(), &["output_tokens"]) + token_count(&normalized, &["output_tokens"]),
        "cache_read_input_tokens": token_count_option(current.as_ref(), &["cache_read_input_tokens"]) + token_count(&normalized, &["cache_read_input_tokens"]),
        "cache_creation_input_tokens": token_count_option(current.as_ref(), &["cache_creation_input_tokens"]) + token_count(&normalized, &["cache_creation_input_tokens"]),
    })
}

fn add_optional_f64(current: Option<f64>, next: Option<f64>) -> Option<f64> {
    match (current, next) {
        (Some(current), Some(next)) => Some(current + next),
        (Some(current), None) => Some(current),
        (None, Some(next)) => Some(next),
        (None, None) => None,
    }
}

fn token_count_option(value: Option<&Value>, names: &[&str]) -> u64 {
    value.map(|value| token_count(value, names)).unwrap_or(0)
}

fn token_count(value: &Value, names: &[&str]) -> u64 {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_u64))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    use super::*;
    use tokio::io::AsyncBufReadExt;

    #[test]
    fn step_finish_reads_metrics_from_part_payload() {
        let mut state = OpenCodeStreamState::default();
        normalize_event_lines(
            &OpenCodeEvent(json!({
                "type": "step_finish",
                "part": {
                    "cost": 0.125,
                    "tokens": {
                        "input": 12,
                        "output": 7,
                        "cache_read": 3,
                        "cache_creation": 2
                    }
                }
            })),
            &mut state,
        );

        let completion = result_stream_line(
            state,
            Ok(std::process::ExitStatus::from_raw(0)),
            None,
            "",
            10,
            false,
        );

        assert_eq!(completion.cost_usd, Some(0.125));
        assert_eq!(completion.result_line["total_cost_usd"], json!(0.125));
        assert_eq!(completion.result_line["usage"]["input_tokens"], json!(12));
        assert_eq!(completion.result_line["usage"]["output_tokens"], json!(7));
        assert_eq!(
            completion.result_line["usage"]["cache_read_input_tokens"],
            json!(3)
        );
        assert_eq!(
            completion.result_line["usage"]["cache_creation_input_tokens"],
            json!(2)
        );
    }

    #[test]
    fn step_finish_accumulates_usage_across_steps() {
        let mut state = OpenCodeStreamState::default();
        for (cost, input, output) in [(0.25, 10, 3), (0.5, 8, 5)] {
            normalize_event_lines(
                &OpenCodeEvent(json!({
                    "type": "step_finish",
                    "part": {
                        "cost": cost,
                        "tokens": {
                            "input": input,
                            "output": output
                        }
                    }
                })),
                &mut state,
            );
        }

        let completion = result_stream_line(
            state,
            Ok(std::process::ExitStatus::from_raw(0)),
            None,
            "",
            10,
            false,
        );

        assert_eq!(completion.cost_usd, Some(0.75));
        assert_eq!(completion.result_line["usage"]["input_tokens"], json!(18));
        assert_eq!(completion.result_line["usage"]["output_tokens"], json!(8));
    }

    #[test]
    fn tool_event_preserves_input_and_output_payloads() {
        let mut state = OpenCodeStreamState::default();
        let lines = normalize_event_lines(
            &OpenCodeEvent(json!({
                "type": "tool",
                "part": {
                    "tool": "bash",
                    "callID": "tool-1",
                    "state": {
                        "input": {"command": "pwd"},
                        "output": "/tmp/worktree\n"
                    }
                }
            })),
            &mut state,
        );

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["message"]["content"][0]["id"], json!("tool-1"));
        assert_eq!(
            lines[0]["message"]["content"][0]["input"]["command"],
            json!("pwd")
        );
        assert_eq!(lines[1]["content"], json!("/tmp/worktree\n"));
    }

    #[tokio::test]
    async fn empty_resume_stdout_uses_fallback_session_id() {
        let mut child = tokio::process::Command::new("sh")
            .arg("-c")
            .arg("true")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();

        let (session_id, buffered) = read_until_session_id(&mut lines, Some("existing-session"))
            .await
            .unwrap();

        assert_eq!(session_id, "existing-session");
        assert!(buffered.is_empty());
        assert!(child.wait().await.unwrap().success());
    }

    #[test]
    fn expected_termination_records_interrupted_non_clean_result() {
        let completion = result_stream_line(
            OpenCodeStreamState::default(),
            Ok(std::process::ExitStatus::from_raw(15)),
            Some("read interrupted".to_string()),
            "Terminated",
            10,
            true,
        );

        assert_eq!(
            completion.final_status,
            global_types::SessionStatus::Stopped
        );
        assert_eq!(completion.result_line["is_error"], json!(false));
        assert_eq!(completion.result_line["subtype"], json!("interrupted"));
        assert_ne!(completion.result_line["subtype"], json!("success"));
        assert_eq!(completion.result_line["error"], Value::Null);
        assert_eq!(
            completion.result_line["result"],
            json!("OpenCode session stopped before completion")
        );
    }
}
