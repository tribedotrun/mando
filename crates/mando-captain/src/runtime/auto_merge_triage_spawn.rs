//! Spawn logic for auto-merge triage sessions.

use std::panic::AssertUnwindSafe;

use anyhow::Result;
use futures::FutureExt;
use rustc_hash::FxHashMap;
use tracing::{info, warn};

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::Task;

use super::auto_merge_triage::triage_json_schema;
use super::notify::Notifier;

/// Spawn a triage CC session for an item. Item stays in AwaitingReview.
pub(crate) async fn spawn_triage(
    item: &mut Task,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let cwd = item
        .worktree
        .as_deref()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            mando_config::resolve_project_config(Some(&item.project), config)
                .map(|(_, pc)| std::path::PathBuf::from(&pc.path))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no CWD for auto-merge triage: no worktree and no project path for {:?}",
                item.project
            )
        })?;

    let pr_num = item
        .pr_number
        .ok_or_else(|| anyhow::anyhow!("cannot triage item without a PR"))?;

    let repo = item
        .github_repo
        .clone()
        .or_else(|| mando_config::resolve_github_repo(Some(&item.project), config))
        .ok_or_else(|| anyhow::anyhow!("no github_repo for project {:?}", item.project))?;

    let pr_number_str = pr_num.to_string();
    let pr_url = format!("https://github.com/{repo}/pull/{pr_number_str}");
    let branch = item.branch.clone().unwrap_or_default();

    // Build evidence file listing from DB artifacts.
    let artifacts = mando_db::queries::artifacts::list_for_task(pool, item.id)
        .await
        .unwrap_or_default();
    let data_dir = mando_types::data_dir();
    let mut evidence_listing = String::new();
    for artifact in &artifacts {
        if artifact.artifact_type == mando_types::ArtifactType::Evidence {
            for media in &artifact.media {
                if let Some(ref local) = media.local_path {
                    let caption = media.caption.as_deref().unwrap_or("(no caption)");
                    evidence_listing.push_str(&format!(
                        "- {} ({})\n",
                        data_dir.join(local).display(),
                        caption
                    ));
                }
            }
        }
    }

    // Latest work summary.
    let work_summary = artifacts
        .iter()
        .rfind(|a| a.artifact_type == mando_types::ArtifactType::WorkSummary)
        .map(|a| a.content.clone())
        .unwrap_or_default();

    // Fetch PR metadata from GitHub (body + changed files).
    let (pr_body, changed_files) = fetch_pr_metadata(&repo, &pr_number_str).await;

    // Render prompt.
    let title = item.title.clone();
    let context = item.context.clone().unwrap_or_default();
    let original_prompt = item.original_prompt.clone().unwrap_or_default();

    let mut vars: FxHashMap<&str, String> = FxHashMap::default();
    vars.insert("title", title.clone());
    vars.insert("context", context);
    vars.insert("original_prompt", original_prompt);
    vars.insert("pr_url", pr_url.clone());
    vars.insert("pr_number", pr_number_str.clone());
    vars.insert("branch", branch);
    vars.insert("evidence_files", evidence_listing);
    vars.insert("work_summary", work_summary);
    vars.insert("changed_files", changed_files);
    vars.insert("pr_body", pr_body);

    let prompt = mando_config::render_prompt("auto_merge_triage", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("render auto_merge_triage prompt: {e}"))?;

    // Generate session ID and store it.
    let session_id = mando_uuid::Uuid::v4().to_string();
    item.session_ids.triage = Some(session_id.clone());
    item.last_activity_at = Some(mando_types::now_rfc3339());

    let task_id = item.id.to_string();
    let task_id_num = item.id;

    // Pick a credential for load balancing.
    let credential = super::tick_spawn::pick_credential(pool, None).await;
    let cred_id = credential.as_ref().map(|c| c.0);

    // Log running session eagerly.
    if let Err(e) = crate::io::headless_cc::log_running_session(
        pool,
        &session_id,
        &cwd,
        "auto-merge-triage",
        "",
        Some(item.id),
        false,
        cred_id,
    )
    .await
    {
        warn!(module = "captain", %session_id, %e, "failed to log running triage session");
    }

    let escaped_title = mando_shared::telegram_format::escape_html(&title);
    notifier
        .normal(&format!(
            "\u{1f50e} Auto-merge triage started for <b>{escaped_title}</b>"
        ))
        .await;

    info!(
        module = "captain",
        item_id = item.id,
        %session_id,
        "spawning auto-merge triage session"
    );

    let captain_model = workflow.models.captain.clone();
    let timeout = workflow.agent.captain_review_timeout_s;
    let pool = pool.clone();
    let triage_notifier = notifier.fork();

    let session_id_for_panic = session_id.clone();
    tokio::spawn(async move {
        let result = AssertUnwindSafe(async move {
            let builder = mando_cc::CcConfig::builder()
                .model(&captain_model)
                .timeout(timeout)
                .caller("auto-merge-triage")
                .task_id(&task_id)
                .cwd(cwd.clone())
                .session_id(session_id.clone())
                .allowed_tools(vec!["Read".into(), "Bash".into()])
                .disallowed_tools(vec!["Agent".into()])
                .json_schema(triage_json_schema());
            let config = super::tick_spawn::with_credential(builder, &credential).build();

            let sid_for_hook = session_id.clone();
            match mando_cc::CcOneShot::run_with_pid_hook(&prompt, config, |pid| {
                if let Err(e) = crate::io::pid_registry::register(&sid_for_hook, pid) {
                    warn!(module = "captain", sid = %sid_for_hook, %e, "pid_registry register failed");
                }
            })
            .await
            {
                Ok(result) => {
                    info!(module = "captain", %session_id, "auto-merge triage CC completed");
                    if let Err(e) = crate::io::pid_registry::unregister(&session_id) {
                        warn!(module = "captain", %session_id, %e, "pid_registry unregister failed");
                    }
                    let cred_id =
                        mando_db::queries::sessions::get_credential_id(&pool, &session_id)
                            .await
                            .unwrap_or(None);
                    triage_notifier
                        .check_rate_limit(&result, &pool, cred_id)
                        .await;
                    if let Err(e) = crate::io::headless_cc::log_cc_result(
                        &pool,
                        &result,
                        &cwd,
                        "auto-merge-triage",
                        Some(task_id_num),
                    )
                    .await
                    {
                        warn!(module = "captain", %session_id, %e, "log_cc_result failed");
                    }
                }
                Err(e) => {
                    warn!(module = "captain", %session_id, %e, "auto-merge triage CC failed");
                    if let Err(e2) = crate::io::pid_registry::unregister(&session_id) {
                        warn!(module = "captain", %session_id, %e2, "pid_registry unregister failed");
                    }
                    let stream_path = mando_config::stream_path_for_session(&session_id);
                    mando_cc::write_error_result(
                        &stream_path,
                        &format!("auto-merge triage CC process failed: {e}"),
                    );
                    if let Err(e2) = crate::io::headless_cc::log_cc_failure(
                        &pool,
                        &session_id,
                        &cwd,
                        "auto-merge-triage",
                        Some(task_id_num),
                    )
                    .await
                    {
                        warn!(module = "captain", %session_id, %e2, "log_cc_failure failed");
                    }
                }
            }
        })
        .catch_unwind()
        .await;

        if let Err(panic) = result {
            tracing::error!(
                module = "captain",
                session_id = %session_id_for_panic,
                "auto-merge triage spawn panicked: {:?}",
                panic
            );
            let stream_path = mando_config::stream_path_for_session(&session_id_for_panic);
            mando_cc::write_error_result(
                &stream_path,
                &format!("auto-merge triage spawn panicked: {:?}", panic),
            );
        }
    });

    Ok(())
}

/// Fetch PR body and changed files list from GitHub via `gh pr view`.
async fn fetch_pr_metadata(repo: &str, pr_number: &str) -> (String, String) {
    let output = tokio::process::Command::new("gh")
        .args([
            "pr",
            "view",
            pr_number,
            "--repo",
            repo,
            "--json",
            "body,files",
        ])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => {
            let json: serde_json::Value =
                serde_json::from_slice(&o.stdout).unwrap_or(serde_json::Value::Null);
            let body = json
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let files = json
                .get("files")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            (body, files)
        }
        Ok(o) => {
            tracing::debug!(
                module = "captain",
                stderr = %String::from_utf8_lossy(&o.stderr),
                "gh pr view failed for triage metadata"
            );
            (String::new(), String::new())
        }
        Err(e) => {
            tracing::debug!(module = "captain", error = %e, "gh pr view command failed");
            (String::new(), String::new())
        }
    }
}
