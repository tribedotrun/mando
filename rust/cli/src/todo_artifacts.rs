//! `mando todo evidence` and `mando todo summary` -- artifact CLI commands.

use std::io::IsTerminal;

use crate::gateway_paths as paths;
use crate::http::{parse_id, DaemonClient};
use crate::motion_check::{check_video, Verdict};

fn parse_evidence_kind(raw: &str) -> anyhow::Result<Option<api_types::EvidenceKind>> {
    match raw {
        "" => Ok(None),
        "before" | "before_fix" => Ok(Some(api_types::EvidenceKind::BeforeFix)),
        "after" | "after_fix" => Ok(Some(api_types::EvidenceKind::AfterFix)),
        "cannot-reproduce" | "cannot_reproduce" => {
            Ok(Some(api_types::EvidenceKind::CannotReproduce))
        }
        "other" => Ok(Some(api_types::EvidenceKind::Other)),
        other => anyhow::bail!(
            "invalid --kind value `{other}`: expected one of `before`, `after`, `cannot-reproduce`, `other`"
        ),
    }
}

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

pub(crate) async fn handle_evidence(
    files: &[String],
    captions: &[String],
    kinds: &[String],
    allow_static: bool,
) -> anyhow::Result<()> {
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
    if !kinds.is_empty() && kinds.len() != files.len() {
        anyhow::bail!(
            "kind count ({}) must match file count ({}) when --kind is passed",
            kinds.len(),
            files.len()
        );
    }
    let parsed_kinds: Vec<Option<api_types::EvidenceKind>> = if kinds.is_empty() {
        vec![None; files.len()]
    } else {
        kinds
            .iter()
            .map(|k| parse_evidence_kind(k))
            .collect::<anyhow::Result<Vec<_>>>()?
    };

    // Reject visually static recordings before registering anything with the
    // daemon. Captain enforces "UI changes need a recording", but a recording
    // whose frames don't move is not real evidence (PR #977 #992). This is
    // the authoritative gate -- catches recordings from any source, not just
    // the in-tree mando-dev recorder.
    if !allow_static {
        for source_path in files {
            let p = std::path::Path::new(source_path);
            let ext = p
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if !matches!(ext.as_str(), "mp4" | "mov" | "webm") {
                continue;
            }
            match check_video(p) {
                Ok(v) if v.verdict == Verdict::Degenerate => {
                    anyhow::bail!(
                        "evidence rejected: {}\n  file: {}\n  Pass --allow-static when you really mean to ship a recording of nothing happening.",
                        v.reason,
                        source_path,
                    );
                }
                Ok(v) => {
                    tracing::debug!(
                        file = %source_path,
                        changed_fraction = v.changed_fraction,
                        pairs = v.sampled_pairs.len(),
                        "motion check ok"
                    );
                }
                Err(e) => {
                    anyhow::bail!(
                        "motion check failed on {}: {}\n  Inspect the file with `ffprobe` or pass --allow-static to bypass.",
                        source_path,
                        e
                    );
                }
            }
        }
    }

    let task_id = resolve_task_id_from_env(None)?;
    let client = DaemonClient::discover()?;
    let data_dir = crate::http::data_dir();

    let file_inputs: Vec<api_types::EvidenceFileRequest> = files
        .iter()
        .zip(captions.iter())
        .zip(parsed_kinds.iter())
        .map(|((path, caption), kind)| {
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
                kind: *kind,
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
        let result = std::process::Command::new("ffmpeg")
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
            .stderr(std::process::Stdio::piped())
            .output();
        match result {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                // ffmpeg exited nonzero. Capture the actual diagnostic --
                // previously this was routed to /dev/null and only a debug
                // log fired off the cleanup path, hiding ffmpeg breakage
                // (missing codec, unreadable input) behind a silent best-
                // effort. Frame extraction stays best-effort, but the
                // operator now has the stderr in the structured log.
                let stderr = String::from_utf8_lossy(&o.stderr);
                tracing::warn!(
                    video = %video_path,
                    ts,
                    exit_code = o.status.code().unwrap_or(-1),
                    stderr = %stderr.trim(),
                    "ffmpeg failed to extract evidence preview frame",
                );
                if let Err(e) = std::fs::remove_file(&frame_path) {
                    tracing::debug!(
                        path = %frame_path.display(),
                        error = %e,
                        "failed to remove partial screenshot frame",
                    );
                }
            }
            Err(e) => {
                // ffmpeg could not be spawned at all (binary missing or
                // permission denied). Surface the spawn error rather than
                // silently dropping it.
                tracing::warn!(
                    video = %video_path,
                    ts,
                    error = %e,
                    "ffmpeg spawn failed for evidence preview frame",
                );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::Mutex;

    /// `MANDO_TASK_ID` and `current_dir()` are process-global. nextest runs
    /// tests in threads inside the same process by default, so two tests
    /// touching either at the same time can corrupt each other's
    /// `resolve_task_id_from_env` call. Take this lock around any test that
    /// reads or mutates env / cwd.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn ffmpeg_available() -> bool {
        Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn make_static_webm(path: &std::path::Path) {
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=0x808080:s=320x240:d=3",
                "-c:v",
                "libvpx-vp9",
                "-crf",
                "30",
                "-b:v",
                "0",
                "-pix_fmt",
                "yuv420p",
                &path.to_string_lossy(),
            ])
            .status()
            .expect("ffmpeg static webm build");
        assert!(status.success(), "static webm build failed");
    }

    // Holding `std::sync::Mutex` across `await` is intentional here: the lock
    // serializes test threads that touch process-global env/cwd, and tokio
    // tests in this file run on the current-thread runtime, so there is no
    // executor that could re-enter the lock.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn rejects_static_recording_before_touching_daemon() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg not available");
            return;
        }
        // Acquired even though this test does not mutate env/cwd: pairs
        // with the lock held by `allow_static_bypasses_motion_check` so
        // their threads do not interleave a `set_current_dir` underneath
        // each other's `handle_evidence` call.
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");

        // Build a static webm and call handle_evidence with allow_static=false.
        // The motion check fires before the daemon client is constructed, so
        // this test does not need a running daemon; failure proves the order.
        let dir = std::env::temp_dir().join(format!(
            "mando-cli-evidence-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let webm: PathBuf = dir.join("static.webm");
        make_static_webm(&webm);

        let webm_str = webm.to_string_lossy().into_owned();
        let result = handle_evidence(
            std::slice::from_ref(&webm_str),
            &["caption".to_string()],
            &[],
            false,
        )
        .await;
        global_infra::best_effort!(
            std::fs::remove_dir_all(&dir),
            "cleanup rejects_static test dir"
        );

        let err = result.expect_err("handle_evidence should reject static webm");
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("evidence rejected"),
            "expected 'evidence rejected' in error, got: {msg}"
        );
        assert!(
            msg.contains(&webm_str),
            "expected error to name the rejected file, got: {msg}"
        );
        assert!(
            msg.contains("changed_fraction"),
            "expected error to include changed_fraction percentage, got: {msg}"
        );
    }

    // See `rejects_static_recording_before_touching_daemon` — same rationale
    // for holding `std::sync::Mutex` across `await`.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn allow_static_bypasses_motion_check() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg not available");
            return;
        }
        // Serialize against any other test in this module that touches
        // process-global state (env, cwd). nextest runs tests as threads
        // by default; without this lock `set_current_dir` from one test
        // could corrupt another's `resolve_task_id_from_env`.
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");

        // With --allow-static, motion check is skipped; the next failure
        // surfaces from the daemon client (or env resolution) instead. We
        // assert that the failure is *not* the motion-check rejection.
        let dir = std::env::temp_dir().join(format!(
            "mando-cli-evidence-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() + 1)
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let webm: PathBuf = dir.join("static.webm");
        make_static_webm(&webm);

        let webm_str = webm.to_string_lossy().into_owned();
        // Force task-id resolution to fail deterministically by clearing env
        // and running from a path with no `-todo-` segment. handle_evidence
        // then returns the env error, not a motion-check error.
        let prev_task_id = std::env::var("MANDO_TASK_ID").ok();
        std::env::remove_var("MANDO_TASK_ID");
        let prev_cwd = std::env::current_dir().ok();
        std::env::set_current_dir(&dir).expect("cwd");

        let result = handle_evidence(
            std::slice::from_ref(&webm_str),
            &["caption".to_string()],
            &[],
            true,
        )
        .await;

        if let Some(cwd) = prev_cwd {
            global_infra::best_effort!(std::env::set_current_dir(cwd), "restore test cwd");
        }
        if let Some(prev) = prev_task_id {
            std::env::set_var("MANDO_TASK_ID", prev);
        }
        global_infra::best_effort!(
            std::fs::remove_dir_all(&dir),
            "cleanup allow_static test dir"
        );

        let err = result.expect_err("handle_evidence should still error w/o task id");
        let msg = format!("{:#}", err);
        assert!(
            !msg.contains("evidence rejected"),
            "allow_static should skip motion check, but got: {msg}"
        );
    }
}
