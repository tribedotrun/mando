//! Q&A response parsing and prompt rendering helpers.

use mando_config::workflow::ScoutWorkflow;
use tracing::warn;

use super::qa::QaResult;

/// JSON schema for structured Q&A responses.
pub(super) fn qa_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "answer": { "type": "string" },
            "suggested_followups": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "required": ["answer", "suggested_followups"]
    })
}

pub(super) fn render_first_turn_prompt(
    question: &str,
    summary: &str,
    article: &str,
    raw_content_note: Option<&str>,
    workflow: &ScoutWorkflow,
) -> anyhow::Result<String> {
    let raw_note = raw_content_note.unwrap_or("");
    let user_context_rendered = workflow.user_context.render();

    let mut vars: rustc_hash::FxHashMap<&str, &str> = rustc_hash::FxHashMap::default();
    vars.insert("question", question);
    vars.insert("summary", summary);
    vars.insert("article", article);
    vars.insert("raw_content_note", raw_note);
    vars.insert("user_context", user_context_rendered.as_str());

    mando_config::render_prompt("qa", &workflow.prompts, &vars).map_err(|e| anyhow::anyhow!(e))
}

fn extract_followups(val: &serde_json::Value) -> Vec<String> {
    val["suggested_followups"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn parse_qa_result(result: &mando_cc::CcResult, ctx_sid: &str) -> QaResult {
    let make = |answer: String, followups: Vec<String>| QaResult {
        answer,
        session_id: Some(result.session_id.clone()),
        suggested_followups: followups,
        session_reset: false,
        cost_usd: result.cost_usd,
        duration_ms: result.duration_ms,
        credential_id: None,
    };

    if let Some(ref structured) = result.structured {
        let answer = structured["answer"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| {
                warn!(module = "scout-qa", session_id = %ctx_sid, "structured output has no 'answer', falling back to text");
                result.text.clone()
            });
        return make(answer, extract_followups(structured));
    }

    warn!(module = "scout-qa", session_id = %ctx_sid, "no structured output, trying text JSON extraction");
    let parsed = match mando_captain::biz::json_parse::parse_llm_json(&result.text) {
        Ok(v) => v,
        Err(e) => {
            warn!(module = "scout-qa", error = %e, "JSON extraction failed, using raw text");
            return make(result.text.clone(), Vec::new());
        }
    };
    if let Some(answer) = parsed["answer"].as_str() {
        return make(answer.to_string(), extract_followups(&parsed));
    }

    warn!(module = "scout-qa", session_id = %ctx_sid, "JSON extraction failed, using raw text as answer");
    make(result.text.clone(), Vec::new())
}
