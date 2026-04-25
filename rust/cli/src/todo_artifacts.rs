//! `mando todo evidence` and `mando todo summary` -- artifact CLI commands.

use std::io::IsTerminal;

use crate::gateway_paths as paths;
use crate::http::{parse_id, DaemonClient};

/// Resolve task ID from explicit arg, MANDO_TASK_ID env, or CWD worktree path.
pub(crate) fn resolve_task_id_from_env(explicit: Option<&str>) -> anyhow::Result<i64> {
    if let Some(id) = explicit {
        return parse_id(id, "item");
    }
    if let Ok(env_id) = std::env::var("MANDO_TASK_ID") {
        return parse_id(&env_id, "MANDO_TASK_ID");
    }
    // Parse from CWD worktree directory name: <repo>-todo-<id>-<slot>
    let cwd = std::env::current_dir()?;
    let dir_name = cwd.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if let Some(rest) = dir_name.split("-todo-").nth(1) {
        if let Some(id_str) = rest.split('-').next() {
            if let Ok(id) = id_str.parse::<i64>() {
                return Ok(id);
            }
        }
    }
    anyhow::bail!("no task ID: pass it as argument, set MANDO_TASK_ID, or run from a task worktree")
}

pub(crate) async fn handle_summary(
    item_id: Option<&str>,
    file: Option<&str>,
) -> anyhow::Result<()> {
    let task_id = resolve_task_id_from_env(item_id)?;
    let client = DaemonClient::discover()?;

    let content = if let Some(path) = file {
        std::fs::read_to_string(path)?
    } else if std::io::stdin().is_terminal() {
        anyhow::bail!("provide content via --file <path> or pipe to stdin");
    } else {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        buf
    };

    if content.trim().is_empty() {
        anyhow::bail!("summary content is empty");
    }

    let result: api_types::TaskSummaryResponse = client
        .post_json(
            &paths::task_summary(task_id),
            &api_types::TaskSummaryRequest { content },
        )
        .await?;
    let artifact_id = result.artifact_id;
    println!("Saved work summary for task #{task_id} (artifact #{artifact_id})");
    Ok(())
}

pub(crate) async fn handle_evidence(files: &[String], captions: &[String]) -> anyhow::Result<()> {
    if files.is_empty() {
        anyhow::bail!("at least one file required");
    }
    if captions.len() != files.len() {
        anyhow::bail!(
            "caption count ({}) must match file count ({})",
            captions.len(),
            files.len()
        );
    }

    let task_id = resolve_task_id_from_env(None)?;
    let client = DaemonClient::discover()?;
    let data_dir = crate::http::data_dir();

    let file_inputs: Vec<api_types::EvidenceFileRequest> = files
        .iter()
        .zip(captions.iter())
        .map(|(path, caption)| {
            let p = std::path::Path::new(path);
            let filename = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let ext = p
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            api_types::EvidenceFileRequest {
                filename,
                ext,
                caption: caption.clone(),
            }
        })
        .collect();

    let result: api_types::TaskEvidenceResponse = client
        .post_json(
            &paths::task_evidence(task_id),
            &api_types::TaskEvidenceRequest { files: file_inputs },
        )
        .await?;
    let artifact_id = result.artifact_id;

    let artifacts_dir = data_dir.join("artifacts").join(task_id.to_string());
    std::fs::create_dir_all(&artifacts_dir)?;

    for (i, source_path) in files.iter().enumerate() {
        let local_path = result
            .media
            .get(i)
            .and_then(|m| m.local_path.as_deref())
            .unwrap_or("");
        if !local_path.is_empty() {
            let dest = data_dir.join(local_path);
            std::fs::copy(source_path, &dest)?;
        }
    }

    // Extract video frames for any video files.
    for (i, source_path) in files.iter().enumerate() {
        let ext = std::path::Path::new(source_path)
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if matches!(ext.as_str(), "mp4" | "mov" | "webm") {
            extract_video_frames(source_path, &artifacts_dir, artifact_id, i as u32);
        }
    }

    println!(
        "Saved evidence for task #{task_id} ({} files, artifact #{artifact_id})",
        files.len()
    );
    Ok(())
}

/// Extract frames from a video at 1s, 5s, 10s via ffmpeg.
fn extract_video_frames(
    video_path: &str,
    artifacts_dir: &std::path::Path,
    artifact_id: i64,
    media_index: u32,
) {
    for ts in [1, 5, 10] {
        let frame_path = artifacts_dir.join(format!("{artifact_id}-{media_index}-frame{ts}s.png"));
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-ss",
                &ts.to_string(),
                "-i",
                video_path,
                "-frames:v",
                "1",
                "-y",
                &frame_path.to_string_lossy(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => {}
            _ => {
                // Best-effort cleanup of the tempfile; failure here is
                // secondary to the screenshot tool failure we already
                // handled by skipping the evidence frame.
                if let Err(e) = std::fs::remove_file(&frame_path) {
                    tracing::debug!(
                        path = %frame_path.display(),
                        error = %e,
                        "failed to remove partial screenshot frame",
                    );
                }
            }
        }
    }
}
