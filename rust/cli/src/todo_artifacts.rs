//! `mando todo evidence` and `mando todo summary` -- artifact CLI commands.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sqlx::Connection;

use crate::http::{parse_id, DaemonClient};
use crate::motion_check::{check_video, Verdict};

static EVIDENCE_STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct TaskWorktreeRef {
    id: i64,
    worktree: Option<String>,
}

impl From<&api_types::TaskItem> for TaskWorktreeRef {
    fn from(item: &api_types::TaskItem) -> Self {
        Self {
            id: item.id,
            worktree: item.worktree.clone(),
        }
    }
}

fn todo_suffix_task_id(cwd: &Path) -> Option<i64> {
    let dir_name = cwd.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let rest = dir_name.split("-todo-").nth(1)?;
    let id_str = rest.split('-').next()?;
    id_str.parse::<i64>().ok()
}

fn comparable_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn worktree_matches_cwd(worktree: &str, cwd: &Path) -> bool {
    let expanded = global_infra::paths::expand_tilde(worktree);
    comparable_path(&expanded) == comparable_path(cwd)
}

fn task_id_for_matching_worktree(
    tasks: &[TaskWorktreeRef],
    cwd: &Path,
) -> anyhow::Result<Option<i64>> {
    let matches: Vec<i64> = tasks
        .iter()
        .filter(|task| {
            task.worktree
                .as_deref()
                .is_some_and(|worktree| worktree_matches_cwd(worktree, cwd))
        })
        .map(|task| task.id)
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        ids => anyhow::bail!(
            "current directory matches multiple Mando tasks: {}",
            ids.iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn missing_task_id_message(explicit_hint: &str) -> String {
    format!("no task ID: {explicit_hint}, set MANDO_TASK_ID, or run from a Mando task worktree")
}

async fn resolve_task_id(
    client: &DaemonClient,
    explicit: Option<&str>,
    explicit_hint: &str,
) -> anyhow::Result<i64> {
    if let Some(id) = explicit {
        return parse_id(id, "item");
    }
    if let Ok(env_id) = std::env::var("MANDO_TASK_ID") {
        return parse_id(&env_id, "MANDO_TASK_ID");
    }

    let cwd = std::env::current_dir()?;
    let suffix_id = todo_suffix_task_id(&cwd);
    let resp = client
        .get_tasks(&api_types::TaskListQuery {
            include_archived: Some(true),
        })
        .await?;
    let task_refs: Vec<TaskWorktreeRef> = resp.items.iter().map(TaskWorktreeRef::from).collect();
    if let Some(task_id) = task_id_for_matching_worktree(&task_refs, &cwd)? {
        return Ok(task_id);
    }
    if let Some(task_id) = suffix_id {
        if task_refs.iter().any(|task| task.id == task_id) {
            return Ok(task_id);
        }
        anyhow::bail!(
            "worktree name suggests task #{task_id}, but the daemon has no such task and no task has worktree {}",
            cwd.display()
        );
    }
    anyhow::bail!("{}", missing_task_id_message(explicit_hint))
}

/// Resolve task ID from explicit arg, MANDO_TASK_ID env, or CWD worktree path.
pub(crate) fn resolve_task_id_from_env(explicit: Option<&str>) -> anyhow::Result<i64> {
    if let Some(id) = explicit {
        return parse_id(id, "item");
    }
    if let Ok(env_id) = std::env::var("MANDO_TASK_ID") {
        return parse_id(&env_id, "MANDO_TASK_ID");
    }
    let cwd = std::env::current_dir()?;
    if let Some(id) = todo_suffix_task_id(&cwd) {
        return Ok(id);
    }
    anyhow::bail!("{}", missing_task_id_message("pass it as an argument"))
}

async fn ensure_task_exists(client: &DaemonClient, task_id: i64) -> anyhow::Result<()> {
    let resp = client
        .get_tasks(&api_types::TaskListQuery {
            include_archived: Some(true),
        })
        .await?;
    if resp.items.iter().any(|item| item.id == task_id) {
        Ok(())
    } else {
        anyhow::bail!("task #{task_id} not found")
    }
}

async fn resolve_summary_task_id(
    client: &DaemonClient,
    item_id: Option<&str>,
) -> anyhow::Result<i64> {
    if item_id.is_some() || std::env::var("MANDO_TASK_ID").is_ok() {
        let task_id = resolve_task_id_from_env(item_id)?;
        ensure_task_exists(client, task_id).await?;
        return Ok(task_id);
    }
    resolve_task_id(client, None, "pass the task id as an argument").await
}

async fn resolve_evidence_task_id(
    client: &DaemonClient,
    item_id: Option<&str>,
) -> anyhow::Result<i64> {
    if item_id.is_some() || std::env::var("MANDO_TASK_ID").is_ok() {
        let task_id = resolve_task_id_from_env(item_id)?;
        ensure_task_exists(client, task_id).await?;
        return Ok(task_id);
    }
    resolve_task_id(client, None, "pass --task").await
}

async fn create_client_and_resolve_task(
    item_id: Option<&str>,
    for_evidence: bool,
) -> anyhow::Result<(DaemonClient, i64)> {
    let client = DaemonClient::discover()?;
    let task_id = if for_evidence {
        resolve_evidence_task_id(&client, item_id).await?
    } else {
        resolve_summary_task_id(&client, item_id).await?
    };
    Ok((client, task_id))
}

pub(crate) async fn handle_summary(
    item_id: Option<&str>,
    file: Option<&str>,
) -> anyhow::Result<()> {
    let (client, task_id) = create_client_and_resolve_task(item_id, false).await?;

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

    let result = client
        .post_tasks_by_id_summary(
            &api_types::TaskIdParams { id: task_id },
            &api_types::TaskSummaryRequest { content },
        )
        .await?;
    let artifact_id = result.artifact_id;
    println!("Saved work summary for task #{task_id} (artifact #{artifact_id})");
    Ok(())
}

fn preflight_evidence_sources(files: &[String]) -> anyhow::Result<()> {
    for source_path in files {
        let path = Path::new(source_path);
        let file = std::fs::File::open(path)
            .with_context(|| format!("evidence file is not readable: {}", path.display()))?;
        if !file.metadata()?.is_file() {
            anyhow::bail!("evidence source is not a regular file: {}", path.display());
        }
        if path.file_name().is_none_or(|name| name.is_empty()) {
            anyhow::bail!("evidence path has no filename: {}", path.display());
        }
    }
    Ok(())
}

#[derive(Debug)]
struct StagedEvidence {
    directory: PathBuf,
    files: Vec<PathBuf>,
}

impl StagedEvidence {
    fn cleanup(&self) -> anyhow::Result<()> {
        match std::fs::remove_dir_all(&self.directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| {
                format!(
                    "failed to remove staged evidence at {}",
                    self.directory.display()
                )
            }),
        }
    }
}

fn staging_directory(artifacts_dir: &Path) -> PathBuf {
    let sequence = EVIDENCE_STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    artifacts_dir.join(format!(
        ".evidence-staging-{}-{sequence}",
        std::process::id()
    ))
}

fn stage_evidence_sources(
    artifacts_dir: &Path,
    files: &[String],
) -> anyhow::Result<StagedEvidence> {
    let directory = staging_directory(artifacts_dir);
    std::fs::create_dir(&directory).with_context(|| {
        format!(
            "failed to create evidence staging directory {}",
            directory.display()
        )
    })?;

    let mut staged_files = Vec::with_capacity(files.len());
    for (index, source) in files.iter().enumerate() {
        let extension = Path::new(source)
            .extension()
            .filter(|extension| !extension.is_empty())
            .map(|extension| format!(".{}", extension.to_string_lossy()))
            .unwrap_or_default();
        let destination = directory.join(format!("{index}{extension}"));
        if let Err(copy_error) = std::fs::copy(source, &destination) {
            let staged = StagedEvidence {
                directory,
                files: staged_files,
            };
            if let Err(cleanup_error) = staged.cleanup() {
                tracing::error!(
                    error = %cleanup_error,
                    "failed to clean partial evidence staging batch"
                );
            }
            return Err(copy_error).with_context(|| {
                format!(
                    "failed to stage evidence file {} at {}",
                    source,
                    destination.display()
                )
            });
        }
        staged_files.push(destination);
    }

    Ok(StagedEvidence {
        directory,
        files: staged_files,
    })
}

async fn register_staged_evidence<T, F, Fut>(
    staged: StagedEvidence,
    register: F,
) -> anyhow::Result<(StagedEvidence, T)>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    match register().await {
        Ok(result) => Ok((staged, result)),
        Err(register_error) => match staged.cleanup() {
            Ok(()) => Err(register_error.context("evidence metadata registration failed")),
            Err(cleanup_error) => {
                tracing::error!(
                    error = %cleanup_error,
                    "evidence metadata registration and staged-file cleanup both failed"
                );
                Err(register_error.context(format!(
                    "evidence metadata registration failed and staged-file cleanup also failed: {cleanup_error:#}"
                )))
            }
        },
    }
}

fn move_staged_evidence(
    data_dir: &Path,
    media: &[api_types::ArtifactMedia],
    staged: &StagedEvidence,
) -> anyhow::Result<()> {
    if media.len() != staged.files.len() {
        anyhow::bail!(
            "daemon registered {} media entries for {} evidence files",
            media.len(),
            staged.files.len()
        );
    }
    for (source, media_item) in staged.files.iter().zip(media) {
        let local_path = media_item
            .local_path
            .as_deref()
            .filter(|path| !path.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("daemon omitted the local path for {}", source.display())
            })?;
        let destination = data_dir.join(local_path);
        std::fs::rename(source, &destination).with_context(|| {
            format!(
                "failed to move staged evidence file {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }
    Ok(())
}

fn remove_registered_media(data_dir: &Path, media: &[api_types::ArtifactMedia]) {
    for local_path in media.iter().filter_map(|item| item.local_path.as_deref()) {
        let destination = data_dir.join(local_path);
        match std::fs::remove_file(&destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                path = %destination.display(),
                error = %error,
                "failed to remove evidence media after copy failure"
            ),
        }
    }
}

async fn rollback_evidence_artifact(
    data_dir: &Path,
    task_id: i64,
    artifact_id: i64,
) -> anyhow::Result<()> {
    let db_path = data_dir.join("mando.db");
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .with_context(|| format!("failed to open artifact database at {}", db_path.display()))?;
    let result = sqlx::query(
        "DELETE FROM task_artifacts
         WHERE id = ?1 AND task_id = ?2 AND artifact_type = 'evidence'",
    )
    .bind(artifact_id)
    .bind(task_id)
    .execute(&mut connection)
    .await
    .with_context(|| format!("failed to roll back evidence artifact #{artifact_id}"))?;
    if result.rows_affected() != 1 {
        anyhow::bail!(
            "evidence artifact rollback affected {} rows for artifact #{artifact_id}",
            result.rows_affected()
        );
    }
    Ok(())
}

async fn finalize_registered_evidence(
    data_dir: &Path,
    task_id: i64,
    result: &api_types::TaskEvidenceResponse,
    staged: &StagedEvidence,
) -> anyhow::Result<()> {
    let Err(move_error) = move_staged_evidence(data_dir, &result.media, staged) else {
        if let Err(cleanup_error) = staged.cleanup() {
            tracing::warn!(
                error = %cleanup_error,
                "failed to remove empty evidence staging directory"
            );
        }
        return Ok(());
    };

    remove_registered_media(data_dir, &result.media);
    if let Err(cleanup_error) = staged.cleanup() {
        tracing::error!(
            error = %cleanup_error,
            "failed to remove evidence staging files after finalization failure"
        );
    }
    match rollback_evidence_artifact(data_dir, task_id, result.artifact_id).await {
        Ok(()) => Err(move_error.context(format!(
            "evidence finalization failed; rolled back artifact #{}",
            result.artifact_id
        ))),
        Err(rollback_error) => {
            tracing::error!(
                task_id,
                artifact_id = result.artifact_id,
                move_error = %move_error,
                rollback_error = %rollback_error,
                "evidence finalization and metadata rollback both failed"
            );
            Err(move_error.context(format!(
                "evidence finalization failed and artifact #{} rollback also failed: {rollback_error:#}",
                result.artifact_id
            )))
        }
    }
}

pub(crate) async fn handle_evidence(
    item_id: Option<&str>,
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
    preflight_evidence_sources(files)?;

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

    let (client, task_id) = create_client_and_resolve_task(item_id, true).await?;
    let data_dir = crate::http::data_dir();
    let artifacts_dir = data_dir.join("artifacts").join(task_id.to_string());
    std::fs::create_dir_all(&artifacts_dir).with_context(|| {
        format!(
            "failed to create evidence directory {}",
            artifacts_dir.display()
        )
    })?;

    let file_inputs: Vec<api_types::EvidenceFileRequest> = files
        .iter()
        .zip(captions.iter())
        .zip(parsed_kinds.iter())
        .map(|((path, caption), kind)| {
            let path = Path::new(path);
            api_types::EvidenceFileRequest {
                filename: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                ext: path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                caption: caption.clone(),
                kind: *kind,
            }
        })
        .collect();

    let staged = stage_evidence_sources(&artifacts_dir, files)?;
    let (staged, result) = register_staged_evidence(staged, || async {
        client
            .post_tasks_by_id_evidence(
                &api_types::TaskIdParams { id: task_id },
                &api_types::TaskEvidenceRequest { files: file_inputs },
            )
            .await
            .map_err(anyhow::Error::from)
    })
    .await?;
    let artifact_id = result.artifact_id;

    finalize_registered_evidence(&data_dir, task_id, &result, &staged).await?;

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
#[path = "todo_artifacts_m10_tests.rs"]
mod m10_tests;

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

    #[test]
    fn matching_worktree_wins_over_stale_directory_suffix() {
        let dir = std::env::temp_dir().join(format!(
            "hyper-tribe-todo-153-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let tasks = [TaskWorktreeRef {
            id: 149,
            worktree: Some(dir.to_string_lossy().into_owned()),
        }];

        let suffix_id = todo_suffix_task_id(&dir);
        let matched_id = task_id_for_matching_worktree(&tasks, &dir).expect("worktree match");

        global_infra::best_effort!(
            std::fs::remove_dir_all(&dir),
            "cleanup stale suffix resolver test dir"
        );
        assert_eq!(suffix_id, Some(153));
        assert_eq!(matched_id, Some(149));
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
            None,
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
            None,
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
