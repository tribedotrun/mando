//! Dashboard triage — deterministic scoring for pending-review items.

use anyhow::Result;
use api_types::{TriageItemResponse, TriageResponse};
use settings::Config;

use crate::io::task_store::TaskStore;

/// Triage pending-review items — deterministic scoring using file classification.
#[tracing::instrument(skip_all)]
pub async fn triage_pending_review(
    config: &Config,
    store: &TaskStore,
    item_id: Option<&str>,
) -> Result<TriageResponse> {
    let items = store.load_all().await?;

    let pending: Vec<_> = items
        .iter()
        .filter(|it| {
            it.status == crate::ItemStatus::AwaitingReview
                && it.pr_number.is_some()
                && match item_id {
                    Some(id) => it.id.to_string() == *id,
                    None => true,
                }
        })
        .collect();

    if pending.is_empty() {
        return Ok(TriageResponse {
            items: vec![],
            table: String::new(),
        });
    }

    let mut triage_items = Vec::new();
    let mut github_repo_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for it in &pending {
        let project_name = it.project.as_str();
        let project_config = if it.project.is_empty() {
            None
        } else {
            settings::resolve_project_config(Some(&it.project), config).map(|(_, pc)| pc)
        };
        let github_repo = project_config
            .and_then(|pc| pc.github_repo.clone())
            .unwrap_or_default();
        let classify_rules = project_config
            .map(|pc| pc.classify_rules.as_slice())
            .unwrap_or_default();
        let pr_num: i64 = it.pr_number.unwrap_or(0);

        if pr_num == 0 {
            continue;
        }

        let pr_num_str = pr_num.to_string();
        match global_github::fetch_pr_status(&github_repo, &pr_num_str).await {
            Ok(pr_status) => {
                let ti = crate::service::triage::build_triage_item(
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
            Err(e) => {
                tracing::warn!(
                    module = "triage",
                    task_id = it.id,
                    pr = pr_num,
                    repo = %github_repo,
                    error = %e,
                    "fetch_pr_status failed"
                );
                github_repo_map.insert(it.id.to_string(), github_repo.clone());
                triage_items.push(crate::service::triage::TriageItem {
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
                    fetch_error: e.to_string(),
                });
            }
        }
    }

    crate::service::triage::sort_triage_items(&mut triage_items);

    let response_items: Vec<TriageItemResponse> = triage_items
        .iter()
        .map(|ti| {
            let repo = github_repo_map
                .get(&ti.task_id)
                .cloned()
                .unwrap_or_default();
            TriageItemResponse {
                task_id: ti.task_id.clone(),
                pr_number: ti.pr_number,
                project: ti.project.clone(),
                github_repo: repo,
                title: ti.title.clone(),
                fast_track: ti.fast_track,
                cursor_risk: ti.cursor_risk.clone(),
                file_count: ti.file_count,
                fetch_failed: ti.fetch_failed,
                fetch_error: ti.fetch_error.clone(),
                merge_readiness_score: crate::service::triage::merge_readiness_score(ti),
            }
        })
        .collect();

    let table = crate::service::triage::format_triage_table(&triage_items);

    Ok(TriageResponse {
        items: response_items,
        table,
    })
}
