//! On-demand access to image payloads embedded in Claude Code tool results.
//!
//! Transcript event snapshots deliberately omit raw base64 so opening a long
//! session does not push every screenshot through the JSON API. The media
//! endpoint calls this narrow reader only when the renderer displays an image.

use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::Context;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResultImage {
    pub media_type: String,
    pub base64_data: String,
}

/// Return the `image_index`th base64 image carried by a tool result.
///
/// Claude Code writes the tool call and its result on separate JSONL lines.
/// The stable `tool_use_id` is therefore the only identifier needed here; the
/// source path itself has already been resolved from the session id by the
/// sessions runtime.
pub fn load_tool_result_image(
    stream_path: &Path,
    tool_use_id: &str,
    image_index: usize,
) -> anyhow::Result<Option<ToolResultImage>> {
    let file = std::fs::File::open(stream_path)
        .with_context(|| format!("failed to open transcript {}", stream_path.display()))?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.with_context(|| {
            format!(
                "failed to read transcript line from {}",
                stream_path.display()
            )
        })?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(blocks) = value.pointer("/message/content").and_then(|v| v.as_array()) else {
            continue;
        };
        for block in blocks {
            if block.get("type").and_then(|v| v.as_str()) != Some("tool_result")
                || block.get("tool_use_id").and_then(|v| v.as_str()) != Some(tool_use_id)
            {
                continue;
            }
            let Some(children) = block.get("content").and_then(|v| v.as_array()) else {
                return Ok(None);
            };
            let image = children
                .iter()
                .filter(|child| child.get("type").and_then(|v| v.as_str()) == Some("image"))
                .nth(image_index);
            let Some(image) = image else {
                return Ok(None);
            };
            if image.pointer("/source/type").and_then(|v| v.as_str()) != Some("base64") {
                return Ok(None);
            }
            let Some(media_type) = image.pointer("/source/media_type").and_then(|v| v.as_str())
            else {
                return Ok(None);
            };
            let Some(base64_data) = image.pointer("/source/data").and_then(|v| v.as_str()) else {
                return Ok(None);
            };
            return Ok(Some(ToolResultImage {
                media_type: media_type.to_string(),
                base64_data: base64_data.to_string(),
            }));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn loads_requested_image_from_tool_result_without_parsing_other_payloads() {
        let mut stream = tempfile::NamedTempFile::new().unwrap();
        writeln!(stream, "not-json").unwrap();
        writeln!(
            stream,
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"tool-1","content":[{{"type":"text","text":"before"}},{{"type":"image","source":{{"type":"base64","media_type":"image/png","data":"cG5n"}}}},{{"type":"image","source":{{"type":"base64","media_type":"image/jpeg","data":"anBn"}}}}]}}]}}}}"#
        )
        .unwrap();

        let image = load_tool_result_image(stream.path(), "tool-1", 1)
            .unwrap()
            .unwrap();
        assert_eq!(
            image,
            ToolResultImage {
                media_type: "image/jpeg".into(),
                base64_data: "anBn".into(),
            }
        );
    }

    #[test]
    fn rejects_non_base64_image_sources() {
        let mut stream = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            stream,
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"tool-1","content":[{{"type":"image","source":{{"type":"url","media_type":"image/png","data":"https://example.com/x.png"}}}}]}}]}}}}"#
        )
        .unwrap();

        assert!(load_tool_result_image(stream.path(), "tool-1", 0)
            .unwrap()
            .is_none());
    }
}
