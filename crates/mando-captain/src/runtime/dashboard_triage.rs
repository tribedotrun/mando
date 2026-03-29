//! Dashboard triage — deterministic scoring for pending-review items.

use anyhow::Result;
use mando_config::settings::Config;

use crate::io::task_store::TaskStore;

/// Triage pending-review items — deterministic scoring using file classification.
pub async fn triage_pending_review(
    config: &Config,
    store: &TaskStore,
    item_id: Option<&str>,
) -> Result<serde_json::Value> {
    let items = store.load_all().await?;

    let pending: Vec<_> = items
        .iter()
        .filter(|it| {
            it.status == mando_types::task::ItemStatus::AwaitingReview
                && it.pr.is_some()
                && match item_id {
                    Some(id) => it.id.to_string() == *id,
                    None => true,
                }
        })
        .collect();

    if pending.is_empty() {
        return Ok(serde_json::json!({"items": [], "message": "No pending-review items found."}));
    }

    let mut triage_items = Vec::new();
    let mut github_repo_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for it in &pending {
        let project_name = it.project.as_deref().unwrap_or("");
        let project_config = it
            .project
            .as_deref()
            .and_then(|name| mando_config::resolve_project_config(Some(name), config))
            .map(|(_, pc)| pc);
        let github_repo = project_config
            .and_then(|pc| pc.github_repo.clone())
            .unwrap_or_default();
        let classify_rules = project_config
            .map(|pc| pc.classify_rules.as_slice())
            .unwrap_or_default();
        let pr_str = it.pr.as_deref().unwrap_or("");
        let pr_num: i64 = pr_str
            .trim_start_matches('#')
            .trim_start_matches("https://github.com/")
            .rsplit('/')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        if pr_num == 0 {
            continue;
        }

        let pr_num_str = pr_num.to_string();
        match crate::io::github::fetch_pr_status(&github_repo, &pr_num_str).await {
            Ok(pr_status) => {
                let ti = crate::biz::triage::build_triage_item(
                    &it.id.to_string(),
                    pr_num,
                    project_name,
                    &it.title,
                    &pr_status.changed_files,
                    &pr_status.body,
                    classify_rules,
                );
                github_repo_map.insert(it.id.to_string(), github_repo.clone());
                triage_items.push(ti);
            }
            Err(_) => {
                github_repo_map.insert(it.id.to_string(), github_repo.clone());
                triage_items.push(crate::biz::triage::TriageItem {
                    task_id: it.id.to_string(),
                    pr_number: pr_num,
                    project: project_name.to_string(),
                    title: it.title.clone(),
                    files: vec![],
                    file_categories: std::collections::HashMap::new(),
                    fast_track: false,
                    cursor_risk: None,
                    file_count: 0,
                    fetch_failed: true,
                });
            }
        }
    }

    crate::biz::triage::sort_triage_items(&mut triage_items);

    let results: Vec<serde_json::Value> = triage_items
        .iter()
        .map(|ti| {
            let repo = github_repo_map
                .get(&ti.task_id)
                .cloned()
                .unwrap_or_default();
            serde_json::json!({
                "task_id": ti.task_id,
                "pr_number": ti.pr_number,
                "project": ti.project,
                "github_repo": repo,
                "title": ti.title,
                "fast_track": ti.fast_track,
                "cursor_risk": ti.cursor_risk,
                "file_count": ti.file_count,
                "fetch_failed": ti.fetch_failed,
                "merge_readiness_score": crate::biz::triage::merge_readiness_score(ti),
            })
        })
        .collect();

    let table = crate::biz::triage::format_triage_table(&triage_items);

    Ok(serde_json::json!({
        "items": results,
        "table": table,
    }))
}
