//! Tool-use rendering for CC transcript markdown conversion.

use serde_json::Value;

/// Render a single tool_use block as markdown.
pub(super) fn render_tool_use(name: &str, input: &Value, path_prefix: &str) -> String {
    match name {
        "Bash" => {
            let cmd = input_str(input, "command");
            let desc = input_str(input, "description");
            let bg = if input.get("run_in_background").and_then(|v| v.as_bool()) == Some(true) {
                " (bg)"
            } else {
                ""
            };
            let label = if desc.is_empty() {
                String::new()
            } else {
                format!("*{desc}*  ")
            };
            format!("**Bash**{bg}  {label}\n```bash\n{cmd}\n```")
        }
        "Read" => {
            let fp = short_path(&input_str(input, "file_path"), path_prefix);
            let mut extra = String::new();
            if let Some(off) = input.get("offset").and_then(|v| v.as_i64()) {
                extra.push_str(&format!(" L{off}"));
            }
            if let Some(lim) = input.get("limit").and_then(|v| v.as_i64()) {
                extra.push_str(&format!("+{lim}"));
            }
            format!("**Read**  `{fp}`{extra}")
        }
        "Write" => {
            let fp = short_path(&input_str(input, "file_path"), path_prefix);
            let content = input_str(input, "content");
            let lines = content.matches('\n').count() + 1;
            format!("**Write**  `{fp}` ({lines} lines)")
        }
        "Edit" => {
            let fp = short_path(&input_str(input, "file_path"), path_prefix);
            let old = input_str(input, "old_string");
            let new = input_str(input, "new_string");
            let ra = if input.get("replace_all").and_then(|v| v.as_bool()) == Some(true) {
                " (all)"
            } else {
                ""
            };
            let mut diff_lines: Vec<String> = Vec::new();
            for line in old.lines() {
                diff_lines.push(format!("- {line}"));
            }
            for line in new.lines() {
                diff_lines.push(format!("+ {line}"));
            }
            let diff = diff_lines.join("\n");
            format!("**Edit**{ra}  `{fp}`\n```diff\n{diff}\n```")
        }
        "Grep" => {
            let pattern = input_str(input, "pattern");
            let path = short_path(&input_str(input, "path"), path_prefix);
            let glob = input_str(input, "glob");
            let mode = input_str(input, "output_mode");
            let mut extra = String::new();
            if !path.is_empty() {
                extra.push_str(&format!("  path={path}"));
            }
            if !glob.is_empty() {
                extra.push_str(&format!("  glob={glob}"));
            }
            if !mode.is_empty() && mode != "files_with_matches" {
                extra.push_str(&format!("  mode={mode}"));
            }
            format!("**Grep**  `/{pattern}/`{extra}")
        }
        "Glob" => {
            let pattern = input_str(input, "pattern");
            let path = short_path(&input_str(input, "path"), path_prefix);
            let path_part = if path.is_empty() {
                String::new()
            } else {
                format!("  path={path}")
            };
            format!("**Glob**  `{pattern}`{path_part}")
        }
        "Agent" => {
            let desc = input_str(input, "description");
            let prompt = input_str(input, "prompt");
            let subtype = input_str(input, "subagent_type");
            let bg = if input.get("run_in_background").and_then(|v| v.as_bool()) == Some(true) {
                " (bg)"
            } else {
                ""
            };
            let meta = if subtype.is_empty() {
                String::new()
            } else {
                format!("  type={subtype}")
            };
            let prompt_display = truncate_str(&prompt, 500);
            format!("**Agent**{bg}{meta}  *{desc}*\n```\n{prompt_display}\n```")
        }
        "Skill" => {
            let skill = input_str(input, "skill");
            let args = input_str(input, "args");
            let args_part = if args.is_empty() {
                String::new()
            } else {
                format!(" {args}")
            };
            format!("**Skill**  `/{skill}`{args_part}")
        }
        "StructuredOutput" => {
            let inp_str = serde_json::to_string_pretty(input).unwrap_or_default();
            let capped = truncate_str(&inp_str, 10_000);
            format!("**{name}**\n```json\n{capped}\n```")
        }
        _ => {
            let inp_str = serde_json::to_string_pretty(input).unwrap_or_default();
            let truncated = truncate_str(&inp_str, 500);
            format!("**{name}**\n```json\n{truncated}\n```")
        }
    }
}

/// Detect common path prefix from file_path arguments in tool calls.
pub(super) fn detect_path_prefix(messages: &[Value]) -> String {
    let mut paths: Vec<String> = Vec::new();
    for msg in messages {
        let inner = msg.get("message").unwrap_or(msg);
        if let Some(blocks) = inner.get("content").and_then(|c| c.as_array()) {
            for block in blocks {
                if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                    continue;
                }
                let input = block.get("input").unwrap_or(&Value::Null);
                for key in &["file_path", "path"] {
                    if let Some(fp) = input.get(*key).and_then(|v| v.as_str()) {
                        if fp.contains('/') && !fp.starts_with("/tmp") {
                            paths.push(fp.to_string());
                        }
                    }
                }
            }
        }
    }
    if paths.is_empty() {
        return String::new();
    }

    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for p in &paths {
        let parts: Vec<&str> = p.split('/').collect();
        for depth in 3..parts.len().min(10) {
            let candidate = parts[..depth].join("/") + "/";
            *counts.entry(candidate).or_insert(0) += 1;
        }
    }

    let threshold = (paths.len() as f64 * 0.8) as usize;
    let mut candidates: Vec<(&String, &usize)> =
        counts.iter().filter(|(_, &c)| c >= threshold).collect();
    candidates.sort_by_key(|(p, _)| p.len());
    candidates
        .last()
        .map(|(p, _)| (*p).clone())
        .unwrap_or_default()
}

pub(super) fn input_str(input: &Value, key: &str) -> String {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn short_path(fp: &str, prefix: &str) -> String {
    if !prefix.is_empty() && fp.starts_with(prefix) {
        fp[prefix.len()..].to_string()
    } else {
        fp.to_string()
    }
}

/// Truncate a string at a char boundary, appending "…" if truncated.
pub(super) fn truncate_str(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // Find last char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}
