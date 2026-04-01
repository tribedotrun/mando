//! Pattern distiller — replaces the old learn.rs cycle.
//!
//! Queries the decision journal for statistical patterns, feeds them
//! to an LLM for interpretation, and outputs recommendations to
//! knowledge.md, patterns table, and Telegram.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;

use crate::io::journal_types::Pattern;
use crate::io::ops_log;
use mando_cc::{CcConfig, CcOneShot};

/// Run the pattern distiller cycle.
///
/// Accepts `Arc` values to avoid deep-cloning Config/Workflow for the WAL
/// closure. Callers (gateway routes, background tasks) already hold Arcs.
pub async fn run_distiller(
    config: &Arc<Config>,
    workflow: &Arc<CaptainWorkflow>,
    pool: &sqlx::SqlitePool,
) -> Result<DistillerResult> {
    let config = Arc::clone(config);
    let workflow = Arc::clone(workflow);
    let pool = pool.clone();
    ops_log::with_wal_op("distiller", serde_json::json!({}), move || async move {
        run_distiller_inner(&config, &workflow, &pool).await
    })
    .await
}

/// Result from a distiller run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DistillerResult {
    pub patterns_found: usize,
    pub summary: String,
    pub patterns: Vec<Pattern>,
}

async fn run_distiller_inner(
    _config: &Config,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<DistillerResult> {
    let jdb = crate::io::journal::JournalDb::new(pool.clone());

    let (total, successes, failures, unresolved) = jdb.total_counts().await?;

    if total == 0 {
        tracing::info!(module = "distiller", "no decisions in journal, skipping");
        return Ok(DistillerResult {
            patterns_found: 0,
            summary: "No decisions in journal yet.".into(),
            patterns: vec![],
        });
    }

    let action_rule_stats = jdb.stats_by_action_rule(30).await?;
    let escalation_stats = jdb.escalation_stats(30).await?;
    let repeat_failures = jdb.repeat_failures(30, 3).await?;

    // ── 2. Format stats for the LLM ─────────────────────────────────────
    let stats_text = format_stats(
        total,
        successes,
        failures,
        unresolved,
        &action_rule_stats,
        &escalation_stats,
        &repeat_failures,
    );

    tracing::info!(
        module = "distiller",
        total,
        rules = action_rule_stats.len(),
        "gathered journal stats"
    );

    // ── 3. Load existing knowledge ──────────────────────────────────────
    let knowledge_path = mando_config::state_dir().join("knowledge.md");
    let existing_knowledge = match tokio::fs::read_to_string(&knowledge_path).await {
        Ok(s) => s,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(module = "distiller", error = %e, "failed to read knowledge.md");
            }
            String::new()
        }
    };

    // ── 4. Invoke LLM for pattern interpretation ────────────────────────
    let mut vars = std::collections::HashMap::new();
    vars.insert("existing_knowledge", existing_knowledge.as_str());
    vars.insert("stats", stats_text.as_str());

    let prompt = mando_config::render_prompt("pattern_distiller", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let result = CcOneShot::run(
        &prompt,
        CcConfig::builder()
            .model(&workflow.models.captain)
            .timeout(Duration::from_secs(180))
            .caller("captain-distiller")
            .json_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "patterns": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "pattern": { "type": "string" },
                                "signal": { "type": "string" },
                                "recommendation": { "type": "string" },
                                "confidence": { "type": "number" }
                            },
                            "required": ["pattern", "signal", "recommendation"]
                        }
                    }
                },
                "required": ["patterns"]
            }))
            .build(),
    )
    .await?;

    crate::io::headless_cc::log_cc_session(
        pool,
        &crate::io::headless_cc::SessionLogEntry {
            session_id: &result.session_id,
            cwd: std::path::Path::new(""),
            model: &workflow.models.captain,
            caller: "captain-distiller",
            cost_usd: result.cost_usd,
            duration_ms: result.duration_ms,
            resumed: false,
            task_id: "",
            status: mando_types::SessionStatus::Stopped,
            worker_name: "",
        },
    )
    .await;

    // ── 5. Parse LLM output ─────────────────────────────────────────────
    let new_patterns: Vec<LlmPattern> = if let Some(structured) = result.structured {
        let arr = structured
            .get("patterns")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        serde_json::from_value(arr).unwrap_or_else(|e| {
            tracing::warn!(module = "distiller", error = %e, "failed to parse structured patterns from LLM");
            Vec::new()
        })
    } else {
        let raw = result.text.trim();
        match parse_llm_patterns(raw) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(module = "distiller", error = %e, "LLM output parse failed");
                prune_and_log(&jdb).await;
                return Ok(DistillerResult {
                    patterns_found: 0,
                    summary: format!(
                        "Analyzed {total} decisions but LLM output was unparseable: {e}"
                    ),
                    patterns: vec![],
                });
            }
        }
    };

    if new_patterns.is_empty() {
        tracing::info!(module = "distiller", "no actionable patterns found by LLM");
        // Still prune old decisions.
        prune_and_log(&jdb).await;
        return Ok(DistillerResult {
            patterns_found: 0,
            summary: format!(
                "Analyzed {total} decisions ({successes} success, {failures} failure). No new patterns."
            ),
            patterns: vec![],
        });
    }

    // ── 6. Store patterns + append to knowledge.md ──────────────────────
    let mut stored_patterns = Vec::new();
    let mut knowledge_additions = Vec::new();

    for p in &new_patterns {
        let sample = action_rule_stats
            .iter()
            .find(|s| p.signal.contains(&s.rule) || p.pattern.contains(&s.rule))
            .map(|s| s.total)
            .unwrap_or(total);

        let id = jdb
            .insert_pattern(
                &p.pattern,
                &p.signal,
                &p.recommendation,
                p.confidence,
                sample,
            )
            .await?;

        let pat = Pattern {
            id,
            pattern: p.pattern.clone(),
            signal: p.signal.clone(),
            recommendation: p.recommendation.clone(),
            confidence: p.confidence,
            sample_size: sample,
            status: "pending".into(),
            created_at: mando_types::now_rfc3339(),
        };
        knowledge_additions.push(format!(
            "- **{}** (confidence: {:.0}%): {}",
            p.pattern,
            p.confidence * 100.0,
            p.recommendation
        ));
        stored_patterns.push(pat);
    }

    // Append to knowledge.md.
    if !knowledge_additions.is_empty() {
        let mut content = existing_knowledge;
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str("\n## Patterns — ");
        content.push_str(&mando_types::now_rfc3339());
        content.push('\n');
        for line in &knowledge_additions {
            content.push_str(line);
            content.push('\n');
        }
        tokio::fs::write(&knowledge_path, content).await?;
    }

    // ── 7. Prune old decisions ──────────────────────────────────────────
    prune_and_log(&jdb).await;

    let summary = format!(
        "Analyzed {total} decisions ({successes} success, {failures} failure). Found {} new pattern(s).",
        stored_patterns.len()
    );
    tracing::info!(
        module = "distiller",
        patterns = stored_patterns.len(),
        "distiller complete"
    );

    Ok(DistillerResult {
        patterns_found: stored_patterns.len(),
        summary,
        patterns: stored_patterns,
    })
}

// ── Stats formatting ────────────────────────────────────────────────────

fn format_stats(
    total: i64,
    successes: i64,
    failures: i64,
    unresolved: i64,
    action_rule_stats: &[crate::io::journal_types::ActionRuleStats],
    escalation_stats: &[(String, String, i64)],
    repeat_failures: &[(String, String, i64)],
) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let _ = write!(
        out,
        "### Overview\n- Total decisions: {total}\n- Successes: {successes}\n- Failures: {failures}\n- Unresolved: {unresolved}\n\n"
    );

    if !action_rule_stats.is_empty() {
        out.push_str("### Success Rate by Action × Rule\n");
        out.push_str("| Action | Rule | Total | Success Rate |\n");
        out.push_str("|--------|------|-------|--------------|\n");
        for s in action_rule_stats {
            let rule_truncated: String = s.rule.chars().take(60).collect();
            let _ = writeln!(
                out,
                "| {} | {} | {} | {:.0}% |",
                s.action,
                rule_truncated,
                s.total,
                s.success_rate * 100.0
            );
        }
        out.push('\n');
    }

    if !escalation_stats.is_empty() {
        out.push_str("### Escalation Chains (after failure)\n");
        for (from, to, count) in escalation_stats {
            let _ = writeln!(out, "- {from} → {to}: {count} times");
        }
        out.push('\n');
    }

    if !repeat_failures.is_empty() {
        out.push_str("### Repeat Failures (same worker + action 3+ times)\n");
        for (worker, action, count) in repeat_failures {
            let _ = writeln!(out, "- Worker `{worker}`: {action} failed {count} times");
        }
        out.push('\n');
    }

    out
}

// ── LLM output parsing ─────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct LlmPattern {
    pattern: String,
    signal: String,
    recommendation: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
}

fn default_confidence() -> f64 {
    0.5
}

fn parse_llm_patterns(raw: &str) -> Result<Vec<LlmPattern>> {
    // Try parsing as JSON array directly.
    if let Ok(patterns) = serde_json::from_str::<Vec<LlmPattern>>(raw) {
        return Ok(patterns);
    }
    // Try extracting JSON from markdown code block.
    if let Some(start) = raw.find('[') {
        if let Some(end) = raw.rfind(']') {
            let slice = &raw[start..=end];
            if let Ok(patterns) = serde_json::from_str::<Vec<LlmPattern>>(slice) {
                return Ok(patterns);
            }
        }
    }
    let preview: String = raw.chars().take(200).collect();
    anyhow::bail!("could not parse as JSON pattern array: {preview}")
}

async fn prune_and_log(jdb: &crate::io::journal::JournalDb) {
    match jdb.prune(90).await {
        Ok(pruned) if pruned > 0 => {
            tracing::info!(module = "distiller", pruned, "pruned old journal decisions");
        }
        Err(e) => {
            tracing::warn!(module = "distiller", error = %e, "journal prune failed — old decisions will accumulate");
        }
        _ => {}
    }
}

/// Approve a pending knowledge lesson by ID.
pub async fn approve_knowledge(id: &str) -> Result<()> {
    anyhow::ensure!(
        !id.contains('/') && !id.contains('\\') && !id.contains(".."),
        "invalid lesson id: {id}"
    );
    let state_dir = mando_config::state_dir();
    let knowledge_dir = state_dir.join("knowledge");
    let path = knowledge_dir.join(format!("{id}.json"));

    let data = tokio::fs::read_to_string(&path)
        .await
        .map_err(|_| anyhow::anyhow!("knowledge lesson not found: {id}"))?;
    let mut val: serde_json::Value = serde_json::from_str(&data)?;
    val["status"] = serde_json::json!("approved");
    tokio::fs::write(&path, serde_json::to_string_pretty(&val)?).await?;

    tracing::info!(module = "distiller", lesson_id = %id, "approved knowledge lesson");
    Ok(())
}
