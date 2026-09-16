//! PR file triage and classification rules.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use settings::ClassifyRule;

use crate::service::text::truncate_utf8;

// ── Per-repo file classification rules ───────────────────────────────────

/// Default classification rules used when a project has no custom `classify_rules`.
const DEFAULT_RULES: &[(&str, &[&str])] = &[
    (
        "test",
        &[
            // Rust-style
            "rust/crates/**/tests/**",
            "rust/crates/**/*_tests.rs",
            "rust/crates/**/tests.rs",
            "rust/cli/**/tests/**",
            // Web-style (JS/TS)
            "**/__tests__/**",
            "**/*.test.*",
            "**/*.spec.*",
        ],
    ),
    ("skill", &[".claude/skills/**", ".agents/skills/**"]),
    ("docs", &["*.md", ".ai/plans/**", "CLAUDE.md"]),
    (
        "config",
        &[
            "devtools/scripts/**",
            ".github/**",
            "*.toml",
            "*.cfg",
            "Cargo.lock",
            "rust/Cargo.lock",
            "*.config.*",
            "package.json",
        ],
    ),
];

// ── Cursor risk parsing ──────────────────────────────────────────────────

static CURSOR_RISK_RE: LazyLock<Regex> =
    LazyLock::new(
        || match Regex::new(r"\*\*(?P<risk>Low|Medium|High|Critical)\s+Risk\*\*") {
            Ok(re) => re,
            Err(e) => global_infra::unrecoverable!("CURSOR_RISK_RE compilation failed", e),
        },
    );

static CURSOR_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    match Regex::new(r"(?s)<!--\s*CURSOR_SUMMARY\s*-->(.+?)<!--\s*/CURSOR_SUMMARY\s*-->") {
        Ok(re) => re,
        Err(e) => global_infra::unrecoverable!("CURSOR_BLOCK_RE compilation failed", e),
    }
});

/// Risk level sort order (lower = safer to merge).
pub const RISK_SORT_ORDER: &[(&str, u8)] =
    &[("Low", 0), ("Medium", 1), ("High", 2), ("Critical", 3)];

pub(crate) fn risk_sort_value(risk: &str) -> u8 {
    RISK_SORT_ORDER
        .iter()
        .find(|(k, _)| *k == risk)
        .map(|(_, v)| *v)
        .unwrap_or(99)
}

// ── Core types ───────────────────────────────────────────────────────────

/// Per-item triage assessment produced by deterministic pre-processing.
#[derive(Debug, Clone)]
pub struct TriageItem {
    pub task_id: String,
    pub pr_number: i64,
    pub project: String,
    pub title: String,
    pub fast_track: bool,
    pub cursor_risk: Option<String>,
    pub file_count: usize,
    pub fetch_failed: bool,
    /// Human-readable reason a fetch failed (empty when `fetch_failed` is false).
    pub fetch_error: String,
}

// ── File classification ──────────────────────────────────────────────────

/// Classify a file path into a category using project-specific or default rules.
///
/// Returns one of: "test", "docs", "config", "skill", "prod".
/// Uses fnmatch-style glob matching (matching the Python implementation).
///
/// When `custom_rules` is non-empty, those rules are used exclusively.
/// Otherwise falls back to `DEFAULT_RULES`.
pub(crate) fn classify_file(path: &str, custom_rules: &[ClassifyRule]) -> &'static str {
    if !custom_rules.is_empty() {
        for rule in custom_rules {
            for pattern in &rule.patterns {
                if glob_match(pattern, path) {
                    return category_to_static(&rule.category);
                }
            }
        }
        return "prod";
    }

    for (category, patterns) in DEFAULT_RULES {
        for pattern in *patterns {
            if glob_match(pattern, path) {
                return category;
            }
        }
    }
    "prod"
}

/// Map a dynamic category string to a static str for known categories.
fn category_to_static(cat: &str) -> &'static str {
    match cat {
        "test" => "test",
        "docs" => "docs",
        "config" => "config",
        "skill" => "skill",
        "prod" => "prod",
        other => {
            tracing::warn!(module = "captain-service-triage", category = %other, "unknown classify_rules category — treating as prod");
            "prod"
        }
    }
}

/// Return true if zero files classify as "prod".
pub(crate) fn is_fast_track(file_categories: &HashMap<String, Vec<String>>) -> bool {
    file_categories.get("prod").is_none_or(|v| v.is_empty())
}

// ── Cursor risk ──────────────────────────────────────────────────────────

/// Extract Cursor risk level from the `CURSOR_SUMMARY` block in the PR body.
pub(crate) fn parse_cursor_risk(pr_body: &str) -> Option<String> {
    let block = CURSOR_BLOCK_RE.captures(pr_body)?;
    let block_text = block.get(1)?.as_str();
    let m = CURSOR_RISK_RE.captures(block_text)?;
    Some(m.name("risk")?.as_str().to_string())
}

// ── Builder ──────────────────────────────────────────────────────────────

/// Build a `TriageItem` from raw PR data.
///
/// `classify_rules` are the per-project custom rules (empty = use defaults).
pub(crate) fn build_triage_item(
    task_id: &str,
    pr_number: i64,
    project_name: &str,
    title: &str,
    files: &[String],
    pr_body: &str,
    classify_rules: &[ClassifyRule],
) -> TriageItem {
    let mut categories: HashMap<String, Vec<String>> = HashMap::new();
    for f in files {
        let cat = classify_file(f, classify_rules);
        categories
            .entry(cat.to_string())
            .or_default()
            .push(f.clone());
    }

    let fast_track = is_fast_track(&categories);

    TriageItem {
        task_id: task_id.to_string(),
        pr_number,
        project: project_name.to_string(),
        title: title.to_string(),
        fast_track,
        cursor_risk: parse_cursor_risk(pr_body),
        file_count: files.len(),
        fetch_failed: false,
        fetch_error: String::new(),
    }
}

// ── Sorting ──────────────────────────────────────────────────────────────

/// Sort: fast-track first, then Cursor risk ascending, then file count ascending.
/// Fetch-failed items sort last.
pub(crate) fn sort_triage_items(items: &mut [TriageItem]) {
    items.sort_by_key(|item| {
        let failed = u8::from(item.fetch_failed);
        let ft = u8::from(!item.fast_track);
        let risk = risk_sort_value(item.cursor_risk.as_deref().unwrap_or(""));
        (failed, ft, risk, item.file_count)
    });
}

// ── AI merge-readiness score ─────────────────────────────────────────────

/// Compute a deterministic merge-readiness score (0–100) for a triage item.
///
/// This provides a baseline score. The full AI-scored version calls headless
/// Claude to refine the score, but this deterministic component feeds into the
/// prompt context and acts as a fallback when AI scoring is unavailable.
pub(crate) fn merge_readiness_score(item: &TriageItem) -> i32 {
    if item.fetch_failed {
        return 0;
    }

    let mut score: i32 = 50;

    // Fast-track items start high.
    if item.fast_track {
        score = 95;
    }

    // Cursor risk penalty.
    if let Some(ref risk) = item.cursor_risk {
        match risk.as_str() {
            "Critical" => score = score.min(30),
            "High" => score -= 20,
            "Medium" => score -= 10,
            "Low" => score += 5,
            _ => {}
        }
    }

    // Large changesets are riskier.
    if item.file_count > 20 {
        score -= 15;
    } else if item.file_count > 10 {
        score -= 5;
    }

    score.clamp(0, 100)
}

// ── Formatting ───────────────────────────────────────────────────────────

/// Render sorted triage items as a markdown pipe table (for CLI/PR use).
pub(crate) fn format_triage_table(items: &[TriageItem]) -> String {
    if items.is_empty() {
        return "No pending-review items found.".to_string();
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "{} Pending-Review Items — Triage Report\n",
        items.len()
    ));
    lines.push("| # | PR | Repo | Title | Fast-Track | Cursor Risk | Files |".to_string());
    lines.push("|---|-----|------|-------|------------|-------------|-------|".to_string());

    for (i, item) in items.iter().enumerate() {
        let project_short = &item.project;
        let ft = if item.fetch_failed {
            "FAIL"
        } else if item.fast_track {
            "Yes"
        } else {
            "No"
        };
        let risk = if item.fetch_failed {
            "FAIL".to_string()
        } else {
            item.cursor_risk
                .as_deref()
                .unwrap_or("\u{2014}")
                .to_string()
        };
        let title = if item.title.len() > 50 {
            format!("{}\u{2026}", truncate_utf8(&item.title, 50))
        } else {
            item.title.clone()
        };
        let files = if item.fetch_failed {
            "?".to_string()
        } else {
            item.file_count.to_string()
        };
        lines.push(format!(
            "| {} | #{} | {} | {} | {} | {} | {} |",
            i + 1,
            item.pr_number,
            project_short,
            title,
            ft,
            risk,
            files
        ));
    }

    // Single pass: split fetch_failed vs fast_track PRs into two lists.
    let mut fast_prs: Vec<String> = Vec::new();
    let mut fail_prs: Vec<String> = Vec::new();
    for it in items {
        if it.fetch_failed {
            fail_prs.push(format!("#{}", it.pr_number));
        } else if it.fast_track {
            fast_prs.push(format!("#{}", it.pr_number));
        }
    }

    if !fast_prs.is_empty() {
        lines.push(format!(
            "\nFast-Track ({}): {}",
            fast_prs.len(),
            fast_prs.join(", ")
        ));
    } else {
        lines.push("\nNo fast-track PRs.".to_string());
    }
    if !fail_prs.is_empty() {
        lines.push(format!(
            "Fetch failed ({}): {}",
            fail_prs.len(),
            fail_prs.join(", ")
        ));
    }

    lines.join("\n")
}

// ── Glob matching (fnmatch-compatible) ───────────────────────────────────

/// Simple fnmatch-style glob matcher.
///
/// Supports: `*` (any chars except `/`), `**` (any chars including `/`),
/// `?` (single char).
fn glob_match(pattern: &str, path: &str) -> bool {
    let re_str = glob_to_regex(pattern);
    Regex::new(&re_str).is_ok_and(|re| re.is_match(path))
}

fn glob_to_regex(pattern: &str) -> String {
    let mut re = String::from("^");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    // ** matches everything including /
                    re.push_str(".*");
                    i += 2;
                    // Skip trailing /
                    if i < chars.len() && chars[i] == '/' {
                        i += 1;
                    }
                } else {
                    // * matches everything except /
                    re.push_str("[^/]*");
                    i += 1;
                }
            }
            '?' => {
                re.push_str("[^/]");
                i += 1;
            }
            '.' | '+' | '^' | '$' | '|' | '(' | ')' | '{' | '}' | '[' | ']' | '\\' => {
                re.push('\\');
                re.push(chars[i]);
                i += 1;
            }
            c => {
                re.push(c);
                i += 1;
            }
        }
    }
    re.push('$');
    re
}

// ── Tests ────────────────────────────────────────────────────────────────
