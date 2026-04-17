//! LLM JSON response parser with 4-step fallback.
//!
//! 1. Strict `serde_json::from_str()`
//! 2. Regex-extract first `{...}` or `[...]` block
//! 3. Common fixups (trailing commas, single quotes, unquoted keys)
//! 4. Empty default `{}`

use regex::Regex;
use std::sync::LazyLock;

static TRAILING_COMMA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",\s*([}\]])").unwrap());

/// Parse LLM output as JSON with progressive fallback.
///
/// Returns the parsed `serde_json::Value` or an error if all strategies fail.
/// Callers decide whether to use a default or propagate the error.
pub fn parse_llm_json(raw: &str) -> Result<serde_json::Value, LlmJsonParseError> {
    // Step 1: strict parse.
    if let Ok(v) = serde_json::from_str(raw) {
        return Ok(v);
    }

    // Step 2: regex-extract first JSON block.
    if let Some(extracted) = extract_json_block(raw) {
        if let Ok(v) = serde_json::from_str(&extracted) {
            return Ok(v);
        }

        // Step 3: apply fixups to extracted block.
        let fixed = apply_fixups(&extracted);
        if let Ok(v) = serde_json::from_str(&fixed) {
            return Ok(v);
        }
    }

    // Step 3b: apply fixups to raw input.
    let fixed = apply_fixups(raw);
    if let Ok(v) = serde_json::from_str(&fixed) {
        return Ok(v);
    }

    // All strategies failed.
    let end = raw.floor_char_boundary(raw.len().min(200));
    let preview = &raw[..end];
    Err(LlmJsonParseError {
        preview: preview.to_string(),
    })
}

/// Error returned when all JSON parse strategies fail.
#[derive(Debug)]
pub struct LlmJsonParseError {
    pub preview: String,
}

impl std::fmt::Display for LlmJsonParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "all JSON parse strategies failed; input preview: {}",
            self.preview
        )
    }
}

impl std::error::Error for LlmJsonParseError {}

/// Parse LLM output, expecting a specific shape.
///
/// Returns `Ok(T)` on success, or `Err` if parsing or deserialization fails.
pub fn parse_llm_json_as<T: serde::de::DeserializeOwned>(raw: &str) -> anyhow::Result<T> {
    let value = parse_llm_json(raw).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(serde_json::from_value(value)?)
}

/// Extract the first `{...}` or `[...]` block from text (handles nesting).
fn extract_json_block(text: &str) -> Option<String> {
    // Find first `{` or `[`.
    let start_idx = text.find(['{', '['])?;
    let open = text.as_bytes()[start_idx] as char;
    let close = if open == '{' { '}' } else { ']' };

    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for (i, ch) in text[start_idx..].char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(text[start_idx..start_idx + i + 1].to_string());
            }
        }
    }

    // Unbalanced — return from start to end as best effort.
    Some(text[start_idx..].to_string())
}

/// Apply common fixups to malformed JSON from LLMs.
fn apply_fixups(text: &str) -> String {
    let mut s = text.to_string();

    // Remove trailing commas before } or ].
    s = TRAILING_COMMA_RE.replace_all(&s, "$1").to_string();

    // Replace single quotes with double quotes (outside of existing double-quoted strings).
    s = fix_single_quotes(&s);

    // Strip markdown code fences.
    s = strip_code_fence(&s).to_string();

    s
}

/// Strip ```…``` markdown code fences from both ends. Drops the optional
/// language tag line after the opening fence (e.g. ```json\n).
fn strip_code_fence(s: &str) -> &str {
    let Some(rest) = s.strip_prefix("```") else {
        return s;
    };
    let after_lang = rest.find('\n').map(|i| &rest[i + 1..]).unwrap_or(rest);
    after_lang.trim_end_matches("```").trim()
}

/// Naively replace single quotes with double quotes when not inside a double-quoted string.
fn fix_single_quotes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_double = false;
    let mut prev = '\0';

    for ch in text.chars() {
        if ch == '"' && prev != '\\' {
            in_double = !in_double;
            out.push(ch);
        } else if ch == '\'' && !in_double {
            out.push('"');
        } else {
            out.push(ch);
        }
        prev = ch;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_parse() {
        let v = parse_llm_json(r#"{"status": "ready"}"#).unwrap();
        assert_eq!(v["status"], "ready");
    }

    #[test]
    fn extract_from_markdown() {
        let raw = r#"Here's the result:
```json
{"status": "ready", "context": "test"}
```"#;
        let v = parse_llm_json(raw).unwrap();
        assert_eq!(v["status"], "ready");
    }

    #[test]
    fn trailing_comma() {
        let v = parse_llm_json(r#"{"status": "ready",}"#).unwrap();
        assert_eq!(v["status"], "ready");
    }

    #[test]
    fn single_quotes() {
        let v = parse_llm_json("{'status': 'ready'}").unwrap();
        assert_eq!(v["status"], "ready");
    }

    #[test]
    fn failure_returns_error() {
        let result = parse_llm_json("this is not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn nested_extraction() {
        let raw = r#"Some text before {"items": [{"title": "foo"}, {"title": "bar"}]} and after"#;
        let v = parse_llm_json(raw).unwrap();
        assert_eq!(v["items"][0]["title"], "foo");
    }

    #[test]
    fn array_extraction() {
        let raw = r#"[{"worker": "w1", "action": "skip"}]"#;
        let v = parse_llm_json(raw).unwrap();
        assert!(v.is_array());
        assert_eq!(v[0]["action"], "skip");
    }
}
