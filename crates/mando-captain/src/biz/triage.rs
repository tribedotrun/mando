//! PR file triage and classification rules.

use std::collections::HashMap;
use std::sync::LazyLock;

use mando_config::settings::ClassifyRule;
use regex::Regex;

use crate::runtime::dashboard::truncate_utf8;

// ── Per-repo file classification rules ───────────────────────────────────

/// Default classification rules used when a project has no custom `classify_rules`.
const DEFAULT_RULES: &[(&str, &[&str])] = &[
    (
        "test",
        &[
            // Rust-style
            "crates/**/tests/**",
            "crates/**/tests/*",
            "crates/**/*_tests.rs",
            "crates/**/tests.rs",
            "cli/**/tests/**",
            "cli/**/tests/*",
            // Web-style (JS/TS)
            "**/__tests__/**",
            "**/*.test.*",
            "**/*.spec.*",
        ],
    ),
    (
        "skill",
        &[
            "ai-kit/skills/**",
            "ai-kit/skills/*",
            ".claude/skills/**",
            ".claude/skills/*",
        ],
    ),
    (
        "docs",
        &["*.md", ".ai/plans/**", ".ai/plans/*", "CLAUDE.md"],
    ),
    (
        "config",
        &[
            "scripts/**",
            "scripts/*",
            ".github/**",
            ".github/*",
            "*.toml",
            "*.cfg",
            "Cargo.lock",
            "*.config.*",
            "package.json",
        ],
    ),
];

// ── Cursor risk parsing ──────────────────────────────────────────────────

static CURSOR_RISK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*(Low|Medium|High|Critical)\s+Risk\*\*").unwrap());

static CURSOR_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<!--\s*CURSOR_SUMMARY\s*-->(.+?)<!--\s*/CURSOR_SUMMARY\s*-->").unwrap()
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
    pub files: Vec<String>,
    pub file_categories: HashMap<String, Vec<String>>,
    pub fast_track: bool,
    pub cursor_risk: Option<String>,
    pub file_count: usize,
    pub fetch_failed: bool,
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
            tracing::warn!(category = %other, "unknown classify_rules category — treating as prod");
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
    Some(m.get(1)?.as_str().to_string())
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
        files: files.to_vec(),
        file_categories: categories,
        fast_track,
        cursor_risk: parse_cursor_risk(pr_body),
        file_count: files.len(),
        fetch_failed: false,
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

    let fast_count = items
        .iter()
        .filter(|it| it.fast_track && !it.fetch_failed)
        .count();
    let fail_count = items.iter().filter(|it| it.fetch_failed).count();

    if fast_count > 0 {
        let fast_prs: Vec<String> = items
            .iter()
            .filter(|it| it.fast_track && !it.fetch_failed)
            .map(|it| format!("#{}", it.pr_number))
            .collect();
        lines.push(format!(
            "\nFast-Track ({}): {}",
            fast_count,
            fast_prs.join(", ")
        ));
    } else {
        lines.push("\nNo fast-track PRs.".to_string());
    }
    if fail_count > 0 {
        let fail_prs: Vec<String> = items
            .iter()
            .filter(|it| it.fetch_failed)
            .map(|it| format!("#{}", it.pr_number))
            .collect();
        lines.push(format!(
            "Fetch failed ({}): {}",
            fail_count,
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

#[cfg(test)]
mod tests {
    use super::*;

    // -- classify_file (default rules) -----------------------------------

    #[test]
    fn classify_default_test_file() {
        assert_eq!(
            classify_file("crates/mando-captain/src/biz/triage_tests.rs", &[]),
            "test"
        );
    }

    #[test]
    fn classify_default_nested_test_file() {
        assert_eq!(
            classify_file("crates/mando-captain/src/biz/tests/integration.rs", &[]),
            "test"
        );
    }

    #[test]
    fn classify_default_prod_file() {
        assert_eq!(
            classify_file("crates/mando-captain/src/biz/triage.rs", &[]),
            "prod"
        );
    }

    #[test]
    fn classify_default_docs() {
        assert_eq!(classify_file("README.md", &[]), "docs");
    }

    #[test]
    fn classify_default_config() {
        assert_eq!(classify_file("scripts/lint.sh", &[]), "config");
    }

    #[test]
    fn classify_default_skill() {
        assert_eq!(classify_file("ai-kit/skills/review/SKILL.md", &[]), "skill");
    }

    #[test]
    fn classify_default_plan_docs() {
        assert_eq!(classify_file(".ai/plans/pr-123/plan.md", &[]), "docs");
    }

    // -- classify_file (custom rules from config) -------------------------

    #[test]
    fn classify_custom_rules_test() {
        let rules = vec![
            ClassifyRule {
                category: "test".into(),
                patterns: vec![
                    "**/__tests__/**".into(),
                    "**/*.test.*".into(),
                    "**/*.spec.*".into(),
                ],
            },
            ClassifyRule {
                category: "docs".into(),
                patterns: vec!["*.md".into(), "docs/**".into()],
            },
        ];
        assert_eq!(classify_file("src/__tests__/foo.ts", &rules), "test");
        assert_eq!(
            classify_file("src/components/Button.spec.tsx", &rules),
            "test"
        );
        assert_eq!(classify_file("src/utils/helper.test.ts", &rules), "test");
        assert_eq!(classify_file("README.md", &rules), "docs");
        // Falls through to "prod" for unmatched files.
        assert_eq!(classify_file("src/main.ts", &rules), "prod");
    }

    #[test]
    fn classify_custom_rules_override_defaults() {
        // Custom rules completely replace default rules.
        let rules = vec![ClassifyRule {
            category: "config".into(),
            patterns: vec!["*.config.*".into(), "package.json".into()],
        }];
        // "scripts/lint.sh" would match default "config" but not custom rules.
        assert_eq!(classify_file("scripts/lint.sh", &rules), "prod");
        assert_eq!(classify_file("jest.config.ts", &rules), "config");
    }

    #[test]
    fn classify_mando_project_rules() {
        // Simulate the classifyRules from config.json for the mando project.
        let rules = vec![
            ClassifyRule {
                category: "test".into(),
                patterns: vec![
                    "crates/**/tests/**".into(),
                    "crates/**/tests/*".into(),
                    "crates/**/*_tests.rs".into(),
                    "crates/**/tests.rs".into(),
                    "cli/**/tests/**".into(),
                    "cli/**/tests/*".into(),
                    "electron/tests/**".into(),
                ],
            },
            ClassifyRule {
                category: "skill".into(),
                patterns: vec!["ai-kit/skills/**".into(), ".claude/skills/**".into()],
            },
            ClassifyRule {
                category: "docs".into(),
                patterns: vec!["*.md".into(), ".ai/plans/**".into(), "docs/**".into()],
            },
            ClassifyRule {
                category: "config".into(),
                patterns: vec![
                    ".github/**".into(),
                    "*.toml".into(),
                    "Cargo.lock".into(),
                    "devtools/**".into(),
                ],
            },
        ];
        // Rust tests
        assert_eq!(
            classify_file("crates/mando-config/src/tests.rs", &rules),
            "test"
        );
        assert_eq!(
            classify_file(
                "crates/mando-captain/src/biz/deterministic_tests.rs",
                &rules
            ),
            "test"
        );
        // Electron tests
        assert_eq!(
            classify_file("electron/tests/integration/tasks.spec.ts", &rules),
            "test"
        );
        // Skills
        assert_eq!(
            classify_file("ai-kit/skills/x-land/SKILL.md", &rules),
            "skill"
        );
        assert_eq!(
            classify_file(".claude/skills/x-pr/pr_status.py", &rules),
            "skill"
        );
        // Docs
        assert_eq!(classify_file("CLAUDE.md", &rules), "docs");
        assert_eq!(classify_file("docs/design-system.md", &rules), "docs");
        // Config
        assert_eq!(classify_file("Cargo.toml", &rules), "config");
        assert_eq!(
            classify_file("devtools/mando-dev/cmd/_app.sh", &rules),
            "config"
        );
        assert_eq!(classify_file(".github/workflows/ci.yml", &rules), "config");
        // Prod (unmatched)
        assert_eq!(
            classify_file("crates/mando-gateway/src/server.rs", &rules),
            "prod"
        );
        assert_eq!(
            classify_file("electron/src/renderer/components/TaskTable.tsx", &rules),
            "prod"
        );
    }

    // -- is_fast_track -------------------------------------------------

    #[test]
    fn fast_track_no_prod() {
        let mut cats = HashMap::new();
        cats.insert(
            "test".to_string(),
            vec!["crates/mando-types/src/tests.rs".to_string()],
        );
        assert!(is_fast_track(&cats));
    }

    #[test]
    fn not_fast_track_with_prod() {
        let mut cats = HashMap::new();
        cats.insert("prod".to_string(), vec!["src/main.rs".to_string()]);
        assert!(!is_fast_track(&cats));
    }

    // -- parse_cursor_risk ---------------------------------------------

    #[test]
    fn parse_cursor_risk_low() {
        let body =
            "Some text\n<!-- CURSOR_SUMMARY -->\n**Low Risk**\n<!-- /CURSOR_SUMMARY -->\nmore";
        assert_eq!(parse_cursor_risk(body), Some("Low".to_string()));
    }

    #[test]
    fn parse_cursor_risk_high() {
        let body = "<!-- CURSOR_SUMMARY -->\n**High Risk** changes\n<!-- /CURSOR_SUMMARY -->";
        assert_eq!(parse_cursor_risk(body), Some("High".to_string()));
    }

    #[test]
    fn parse_cursor_risk_outside_block() {
        // Risk text outside CURSOR_SUMMARY block should be ignored.
        let body = "**Critical Risk**\nno cursor block";
        assert_eq!(parse_cursor_risk(body), None);
    }

    #[test]
    fn parse_cursor_risk_no_block() {
        assert_eq!(parse_cursor_risk("just a normal PR body"), None);
    }

    // -- build_triage_item ---------------------------------------------

    #[test]
    fn build_item_classifies_files() {
        let files = vec![
            "crates/mando-captain/src/biz/triage_tests.rs".to_string(),
            "crates/mando-captain/src/biz/triage.rs".to_string(),
            "README.md".to_string(),
        ];
        let item = build_triage_item("id1", 100, "my-project", "My PR", &files, "", &[]);
        assert_eq!(item.file_count, 3);
        assert!(!item.fast_track); // has a prod file
        assert!(item.file_categories.get("test").unwrap().len() == 1);
        assert!(item.file_categories.get("prod").unwrap().len() == 1);
        assert!(item.file_categories.get("docs").unwrap().len() == 1);
    }

    #[test]
    fn build_item_fast_track_no_prod() {
        let files = vec![
            "crates/mando-captain/src/biz/triage_tests.rs".to_string(),
            "README.md".to_string(),
        ];
        let item = build_triage_item("id2", 101, "my-project", "Docs", &files, "", &[]);
        assert!(item.fast_track);
    }

    // -- sort_triage_items ---------------------------------------------

    #[test]
    fn sort_fast_track_first() {
        let mut items = vec![
            TriageItem {
                task_id: "1".into(),
                pr_number: 100,
                project: "acme/widgets".into(),
                title: "Prod change".into(),
                files: vec!["src/main.rs".into()],
                file_categories: {
                    let mut m = HashMap::new();
                    m.insert("prod".to_string(), vec!["src/main.rs".to_string()]);
                    m
                },
                fast_track: false,
                cursor_risk: Some("Medium".into()),
                file_count: 1,
                fetch_failed: false,
            },
            TriageItem {
                task_id: "2".into(),
                pr_number: 101,
                project: "acme/widgets".into(),
                title: "Test only".into(),
                files: vec!["crates/mando-types/src/tests.rs".into()],
                file_categories: {
                    let mut m = HashMap::new();
                    m.insert(
                        "test".to_string(),
                        vec!["crates/mando-types/src/tests.rs".to_string()],
                    );
                    m
                },
                fast_track: true,
                cursor_risk: None,
                file_count: 1,
                fetch_failed: false,
            },
        ];
        sort_triage_items(&mut items);
        assert!(items[0].fast_track);
        assert!(!items[1].fast_track);
    }

    // -- risk_sort_value -----------------------------------------------

    #[test]
    fn risk_sort_order_values() {
        assert!(risk_sort_value("Low") < risk_sort_value("Medium"));
        assert!(risk_sort_value("Medium") < risk_sort_value("High"));
        assert!(risk_sort_value("High") < risk_sort_value("Critical"));
        assert!(risk_sort_value("Critical") < risk_sort_value(""));
    }

    // -- merge_readiness_score -----------------------------------------

    #[test]
    fn score_fast_track() {
        let item = TriageItem {
            task_id: "a".into(),
            pr_number: 1,
            project: "acme/widgets".into(),
            title: "test only".into(),
            files: vec!["crates/mando-types/src/tests.rs".into()],
            file_categories: {
                let mut m = HashMap::new();
                m.insert(
                    "test".to_string(),
                    vec!["crates/mando-types/src/tests.rs".to_string()],
                );
                m
            },
            fast_track: true,
            cursor_risk: None,
            file_count: 1,
            fetch_failed: false,
        };
        assert_eq!(merge_readiness_score(&item), 95);
    }

    #[test]
    fn score_fetch_failed() {
        let item = TriageItem {
            task_id: "b".into(),
            pr_number: 2,
            project: "x".into(),
            title: "fail".into(),
            files: vec![],
            file_categories: HashMap::new(),
            fast_track: false,
            cursor_risk: None,
            file_count: 0,
            fetch_failed: true,
        };
        assert_eq!(merge_readiness_score(&item), 0);
    }

    #[test]
    fn score_critical_risk_capped() {
        let item = TriageItem {
            task_id: "c".into(),
            pr_number: 3,
            project: "acme/widgets".into(),
            title: "big".into(),
            files: vec!["src/a.rs".into()],
            file_categories: {
                let mut m = HashMap::new();
                m.insert("prod".to_string(), vec!["src/a.rs".to_string()]);
                m
            },
            fast_track: false,
            cursor_risk: Some("Critical".into()),
            file_count: 1,
            fetch_failed: false,
        };
        assert_eq!(merge_readiness_score(&item), 30);
    }

    #[test]
    fn score_large_changeset_penalty() {
        let files: Vec<String> = (0..25).map(|i| format!("src/f{i}.rs")).collect();
        let item = TriageItem {
            task_id: "d".into(),
            pr_number: 4,
            project: "acme/widgets".into(),
            title: "big change".into(),
            files: files.clone(),
            file_categories: {
                let mut m = HashMap::new();
                m.insert("prod".to_string(), files);
                m
            },
            fast_track: false,
            cursor_risk: None,
            file_count: 25,
            fetch_failed: false,
        };
        assert_eq!(merge_readiness_score(&item), 35); // 50 - 15
    }

    // -- formatting ----------------------------------------------------

    #[test]
    fn format_table_empty() {
        assert_eq!(format_triage_table(&[]), "No pending-review items found.");
    }

    // -- glob_match ----------------------------------------------------

    #[test]
    fn glob_star_matches_single_level() {
        assert!(glob_match("tests/*", "tests/foo.py"));
        assert!(!glob_match("tests/*", "tests/sub/foo.py"));
    }

    #[test]
    fn glob_doublestar_matches_deep() {
        assert!(glob_match("tests/**", "tests/sub/deep/foo.py"));
        assert!(glob_match("**/__tests__/**", "src/__tests__/foo.ts"));
    }

    #[test]
    fn glob_extension_pattern() {
        assert!(glob_match("*.md", "README.md"));
        assert!(!glob_match("*.md", "src/README.md"));
    }

    #[test]
    fn glob_dot_pattern() {
        assert!(glob_match("**/*.test.*", "src/utils/helper.test.ts"));
        assert!(glob_match("**/*.spec.*", "src/components/Button.spec.tsx"));
    }
}
