//! Clarification flow — single-turn and multi-turn stateful sessions.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use mando_config::workflow::CaptainWorkflow;
use mando_types::Task;
use tracing::{info, warn};

use mando_cc::{CcConfig, CcOneShot};

pub use super::clarifier_session::ClarifierSessionManager;
use super::clarifier_validate::{build_clarifier_schema, check_repo, retry_with_correction};
use super::task_notes::append_labeled_prompt;
use crate::biz::json_parse::parse_llm_json;

// ── Single-turn (legacy captain tick path) ──────────────────────────

/// Run the clarification flow for a new task (single-turn).
pub(crate) async fn run_clarification(
    item: &Task,
    _linear_cli_path: &str,
    workflow: &CaptainWorkflow,
    config: &mando_config::Config,
    pool: &sqlx::SqlitePool,
) -> Result<ClarifierResult> {
    let projects = &config.captain.projects;
    let prompt = build_clarifier_prompt(item, None, workflow, projects)?;

    // Resolve project cwd so the clarifier can read project files.
    let cwd = resolve_clarifier_cwd(item, config);

    let task_id = item.best_id();
    let valid_names: Vec<String> = projects.values().map(|pc| pc.name.clone()).collect();
    let schema = build_clarifier_schema(&valid_names);

    let result = match CcOneShot::run(
        &prompt,
        CcConfig::builder()
            .model(&workflow.models.clarifier)
            .timeout(Duration::from_secs(workflow.agent.clarifier_timeout_s))
            .caller("clarifier")
            .task_id(&task_id)
            .cwd(cwd.clone())
            .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
            .json_schema(schema.clone())
            .build(),
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            warn!(module = "clarifier", title = %item.title, error = %e, "CC failed");
            return Err(e);
        }
    };

    crate::io::headless_cc::log_cc_session(
        pool,
        &crate::io::headless_cc::SessionLogEntry {
            session_id: &result.session_id,
            cwd: &cwd,
            model: &workflow.models.clarifier,
            caller: "clarifier",
            cost_usd: result.cost_usd,
            duration_ms: result.duration_ms,
            resumed: false,
            task_id: &task_id,
            status: mando_types::SessionStatus::Stopped,
            worker_name: "",
        },
    )
    .await;

    let text = result
        .structured
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| result.text.clone());
    let mut parsed = parse_clarifier_response(&text, &item.title);
    parsed.session_id = Some(result.session_id.clone());

    // Validate: if the LLM returned a repo that isn't a valid project name,
    // resume the session and ask it to fix the output.
    if let Some(bad_repo) = check_repo(&parsed, &valid_names) {
        warn!(
            module = "clarifier",
            repo = %bad_repo,
            title = %&item.title[..item.title.len().min(60)],
            "clarifier returned invalid project name — retrying"
        );
        match retry_with_correction(
            &result.session_id,
            &bad_repo,
            &valid_names,
            &schema,
            &cwd,
            workflow,
            &task_id,
            &item.title,
            pool,
        )
        .await
        {
            Ok(corrected) => return Ok(corrected),
            Err(e) => {
                warn!(
                    module = "clarifier",
                    error = %e,
                    "correction retry failed — clearing invalid repo"
                );
                parsed.repo = None;
            }
        }
    }

    info!(
        module = "clarifier",
        title = %&item.title[..item.title.len().min(60)],
        status = ?parsed.status,
        "clarification complete"
    );
    Ok(parsed)
}

// ── Types ───────────────────────────────────────────────────────────

/// Result of a clarification turn.
pub struct ClarifierResult {
    pub status: ClarifierStatus,
    pub context: String,
    pub questions: Option<String>,
    pub generated_title: Option<String>,
    pub repo: Option<String>,
    pub no_pr: Option<bool>,
    pub resource: Option<String>,
    pub session_id: Option<String>,
    /// True when the deep clarification pass was attempted but failed,
    /// so the result reflects only the shallow pass.
    pub deep_failed: bool,
}

/// Clarifier outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClarifierStatus {
    Ready,
    Clarifying,
    Escalate,
}

// ── Shared helpers ──────────────────────────────────────────────────

/// Resolve the working directory for the clarifier based on the item's project.
pub(crate) fn resolve_clarifier_cwd(
    item: &Task,
    config: &mando_config::Config,
) -> std::path::PathBuf {
    if let Some((_key, proj)) =
        mando_config::resolve_project_config(item.project.as_deref(), config)
    {
        mando_config::expand_tilde(&proj.path)
    } else {
        warn!(
            module = "clarifier",
            project = ?item.project,
            title = %item.title,
            "could not resolve project for clarifier cwd — using data dir as fallback"
        );
        mando_config::data_dir()
    }
}

fn build_clarifier_prompt(
    item: &Task,
    human_input: Option<&str>,
    workflow: &CaptainWorkflow,
    projects: &std::collections::HashMap<String, mando_config::settings::ProjectConfig>,
) -> anyhow::Result<String> {
    let context = item.context.as_deref().unwrap_or("none");
    let repo = item.project.as_deref().unwrap_or("");

    let repo_list = projects
        .values()
        .map(|cfg| {
            if let Some(ref gh) = cfg.github_repo {
                format!("- {} (GitHub: {})", cfg.name, gh)
            } else {
                format!("- {}", cfg.name)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut resource_names: Vec<&str> = vec!["cc"];
    for key in workflow.agent.resource_limits.keys() {
        if key != "cc" {
            resource_names.push(key.as_str());
        }
    }
    resource_names[1..].sort();
    let resource_list = resource_names.join(", ");

    let mut vars = HashMap::new();
    vars.insert("title", item.title.as_str());
    vars.insert("context", context);
    vars.insert("repo", repo);
    vars.insert("repo_list", &repo_list);
    vars.insert("resource_list", &resource_list);

    let mut prompt = mando_config::render_prompt("clarifier", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;
    if let Some(input) = human_input {
        append_labeled_prompt(&mut prompt, "Human provided additional details", input);
    }
    Ok(prompt)
}

pub(crate) fn build_interactive_clarifier_turn_prompt(
    item: &Task,
    workflow: &CaptainWorkflow,
    human_input: Option<&str>,
) -> anyhow::Result<String> {
    let system_prompt =
        mando_config::render_prompt("interactive_clarifier", &workflow.prompts, &HashMap::new())
            .map_err(|e| anyhow::anyhow!(e))?;
    let details = item
        .original_prompt
        .as_deref()
        .unwrap_or(item.context.as_deref().unwrap_or("(none)"));
    let current_context = item.context.as_deref().unwrap_or("(none)");
    let outstanding_questions = item.clarifier_questions.as_deref().unwrap_or("(none)");
    let human_response = human_input.unwrap_or("(none)").trim();

    Ok(format!(
        "{system_prompt}\n\n\
         You MUST respond with JSON only.\n\n\
         Task:\n\
         - Title: {}\n\
         - Details: {}\n\
         - Current Context: {}\n\
         - Outstanding Questions: {}\n\n\
         Human response:\n\
         {}\n\n\
         Update the context and decide if the item is ready or still needs clarification.",
        item.title, details, current_context, outstanding_questions, human_response,
    ))
}

pub(crate) async fn run_deep_clarification(
    item: &Task,
    workflow: &CaptainWorkflow,
    config: &mando_config::Config,
    pool: &sqlx::SqlitePool,
    initial: ClarifierResult,
) -> ClarifierResult {
    let questions = initial.questions.as_deref().unwrap_or("").trim();
    let Some(session_id) = initial.session_id.as_deref() else {
        return initial;
    };
    if questions.is_empty() {
        return initial;
    }

    let cwd = resolve_clarifier_cwd(item, config);
    let mut vars = HashMap::new();
    vars.insert("questions", questions);
    vars.insert("context", initial.context.as_str());

    let prompt = match mando_config::render_prompt("deep_clarifier", &workflow.prompts, &vars) {
        Ok(prompt) => prompt,
        Err(e) => {
            warn!(module = "clarifier", error = %e, "failed to render deep clarifier prompt");
            return ClarifierResult {
                deep_failed: true,
                ..initial
            };
        }
    };

    match CcOneShot::run(
        &prompt,
        CcConfig::builder()
            .model(&workflow.models.clarifier)
            .timeout(Duration::from_secs(workflow.agent.clarifier_timeout_s))
            .caller("deep-clarifier")
            .task_id(item.best_id())
            .cwd(cwd.clone())
            .resume(session_id.to_string())
            .allowed_tools(vec!["Read".into(), "Glob".into(), "Grep".into()])
            .json_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["understood", "escalate"] },
                    "context": { "type": "string" },
                    "questions": { "type": ["string", "null"] }
                },
                "required": ["status", "context"]
            }))
            .build(),
    )
    .await
    {
        Ok(result) => {
            crate::io::headless_cc::log_cc_session(
                pool,
                &crate::io::headless_cc::SessionLogEntry {
                    session_id: &result.session_id,
                    cwd: &cwd,
                    model: &workflow.models.clarifier,
                    caller: "deep-clarifier",
                    cost_usd: result.cost_usd,
                    duration_ms: result.duration_ms,
                    resumed: true,
                    task_id: &item.best_id(),
                    status: mando_types::SessionStatus::Stopped,
                    worker_name: "",
                },
            )
            .await;
            let text = result
                .structured
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| result.text.clone());
            let mut parsed = parse_clarifier_response(&text, &item.title);
            parsed.session_id = Some(result.session_id);
            // Deep clarifier answers questions — it doesn't reassign project.
            // Carry forward the validated values from the initial clarification.
            parsed.repo = initial.repo;
            parsed.no_pr = initial.no_pr.or(parsed.no_pr);
            parsed.resource = initial.resource.or(parsed.resource);
            parsed
        }
        Err(e) => {
            warn!(module = "clarifier", error = %e, "deep clarifier failed — using shallow result");
            ClarifierResult {
                deep_failed: true,
                ..initial
            }
        }
    }
}

pub(crate) fn parse_clarifier_response(text: &str, item_title: &str) -> ClarifierResult {
    let parsed = parse_llm_json(text);

    // If we couldn't parse JSON at all, fail closed — escalate rather than
    // sending an unprepared task to a worker.
    let has_valid_status = parsed["status"].is_string();
    if !has_valid_status {
        warn!(
            module = "clarifier",
            "clarifier returned unparseable response — escalating (fail-closed)"
        );
        return ClarifierResult {
            status: ClarifierStatus::Escalate,
            context: item_title.to_string(),
            questions: Some("Clarifier returned unparseable response".into()),
            generated_title: None,
            repo: None,
            no_pr: None,
            resource: None,
            session_id: None,
            deep_failed: false,
        };
    }

    // has_valid_status guarantees is_string(), so as_str() always succeeds here.
    // If it somehow doesn't (e.g. non-string JSON type slipped through), fail closed.
    let status_str = match parsed["status"].as_str() {
        Some(s) => s,
        None => {
            warn!(
                module = "clarifier",
                raw = %parsed["status"],
                "status field present but not a string — escalating (fail-closed)"
            );
            "escalate"
        }
    };
    let status = match status_str {
        "clarifying" => ClarifierStatus::Clarifying,
        "escalate" => ClarifierStatus::Escalate,
        "answered" => ClarifierStatus::Ready,
        "understood" | "ready" => ClarifierStatus::Ready,
        unknown => {
            warn!(
                module = "clarifier",
                status = %unknown,
                "clarifier returned unknown status — escalating (fail-closed)"
            );
            ClarifierStatus::Escalate
        }
    };

    let context = parsed["context"].as_str().unwrap_or(item_title).to_string();
    let questions = parsed["questions"].as_str().map(String::from);
    let generated_title = parsed["title"]
        .as_str()
        .or_else(|| parsed["generated_title"].as_str())
        .map(String::from);

    let repo = parsed["repo"].as_str().map(String::from);
    let no_pr = parsed["no_pr"].as_bool();
    let resource = parsed["resource"].as_str().map(String::from);

    ClarifierResult {
        status,
        context,
        questions,
        generated_title,
        repo,
        no_pr,
        resource,
        session_id: None,
        deep_failed: false,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_understood_response() {
        let json = r#"{"status":"understood","context":"enriched ctx","questions":null,"generated_title":"Better title"}"#;
        let result = parse_clarifier_response(json, "fallback");
        assert_eq!(result.status, ClarifierStatus::Ready);
        assert_eq!(result.context, "enriched ctx");
        assert!(result.questions.is_none());
        assert_eq!(result.generated_title.as_deref(), Some("Better title"));
    }

    #[test]
    fn parse_escalate_response() {
        let json = r#"{"status":"escalate","context":"ambiguous","questions":"What do you mean?"}"#;
        let result = parse_clarifier_response(json, "fallback");
        assert_eq!(result.status, ClarifierStatus::Escalate);
        assert_eq!(result.questions.as_deref(), Some("What do you mean?"));
    }

    #[test]
    fn parse_clarifying_response() {
        let json = r#"{"status":"clarifying","context":"partial","questions":"What repo?","generated_title":null}"#;
        let result = parse_clarifier_response(json, "fallback");
        assert_eq!(result.status, ClarifierStatus::Clarifying);
        assert_eq!(result.questions.as_deref(), Some("What repo?"));
    }

    #[test]
    fn parse_invalid_json_escalates() {
        let result = parse_clarifier_response("not json", "fallback title");
        assert_eq!(result.status, ClarifierStatus::Escalate);
        assert_eq!(result.context, "fallback title");
    }

    #[test]
    fn parse_code_fenced_json() {
        let fenced = "```json\n{\"status\":\"understood\",\"context\":\"enriched\",\"title\":\"Better\",\"repo\":null,\"resource\":\"cc\",\"no_pr\":false}\n```";
        let result = parse_clarifier_response(fenced, "fallback");
        assert_eq!(result.status, ClarifierStatus::Ready);
        assert_eq!(result.context, "enriched");
        assert_eq!(result.generated_title.as_deref(), Some("Better"));
        assert!(result.repo.is_none());
        assert_eq!(result.no_pr, Some(false));
        assert_eq!(result.resource.as_deref(), Some("cc"));
    }

    #[test]
    fn parse_repo_override() {
        let json = r#"{"status":"understood","context":"ctx","repo":"other/repo","no_pr":true}"#;
        let result = parse_clarifier_response(json, "fallback");
        assert_eq!(result.repo.as_deref(), Some("other/repo"));
        assert_eq!(result.no_pr, Some(true));
    }

    #[test]
    fn build_prompt_without_human_input() {
        let workflow = CaptainWorkflow::compiled_default();
        let projects = HashMap::new();
        let mut item = Task::new("Fix bug");
        item.context = Some("in auth".into());
        let prompt = build_clarifier_prompt(&item, None, &workflow, &projects).unwrap();
        assert!(prompt.contains("Fix bug"));
        assert!(prompt.contains("in auth"));
        assert!(!prompt.contains("Human provided"));
    }

    #[test]
    fn build_prompt_with_human_input() {
        let workflow = CaptainWorkflow::compiled_default();
        let projects = HashMap::new();
        let item = Task::new("Fix bug");
        let prompt =
            build_clarifier_prompt(&item, Some("it's in the login page"), &workflow, &projects)
                .unwrap();
        assert!(prompt.contains("Human provided additional details: it's in the login page"));
    }

    #[tokio::test]
    async fn session_manager_new() {
        let db = mando_db::Db::open_in_memory().await.unwrap();
        let mgr = ClarifierSessionManager::new("sonnet", db.pool().clone());
        assert!(!mgr.has_session("nonexistent"));
    }

    #[test]
    fn parse_unknown_status_escalates() {
        let json = r#"{"status":"pending","context":"ctx"}"#;
        let result = parse_clarifier_response(json, "fallback");
        assert_eq!(result.status, ClarifierStatus::Escalate);
    }

    #[test]
    fn parse_ready_status_accepted() {
        let json = r#"{"status":"ready","context":"ctx"}"#;
        let result = parse_clarifier_response(json, "fallback");
        assert_eq!(result.status, ClarifierStatus::Ready);
    }

    #[test]
    fn repo_list_shows_project_name_not_path_key() {
        let workflow = CaptainWorkflow::compiled_default();
        let mut projects = HashMap::new();
        projects.insert(
            "/code/mando".to_string(),
            mando_config::settings::ProjectConfig {
                name: "mando".into(),
                path: "/code/mando".into(),
                github_repo: Some("acme/widgets".into()),
                ..Default::default()
            },
        );
        let item = Task::new("Fix bug");
        let prompt = build_clarifier_prompt(&item, None, &workflow, &projects).unwrap();
        // Must show project name first, GitHub slug labeled.
        assert!(prompt.contains("- mando (GitHub: acme/widgets)"));
        // Must NOT show the raw path key as the primary identifier.
        assert!(!prompt.contains("- /code/mando"));
    }
}
