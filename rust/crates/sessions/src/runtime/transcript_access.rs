use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Context;
use sqlx::SqlitePool;

/// Snapshot payload for `/api/sessions/{id}/events` + starting cursor for the
/// SSE tail loop. The `source` + `byte_offset` let the stream handler
/// resume parsing new JSONL lines without re-reading history.
pub struct EventsSnapshot {
    pub events: Vec<api_types::TranscriptEvent>,
    pub is_running: bool,
    pub source: Option<TranscriptSource>,
    pub byte_offset: u64,
    pub next_line: u32,
}

#[derive(Clone)]
pub struct TranscriptSource {
    pub path: PathBuf,
    pub format: TranscriptFormat,
    pub meta_path: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub enum TranscriptFormat {
    ClaudeCode,
    CodexAppServer,
}

#[tracing::instrument(skip_all)]
pub async fn load_events_snapshot(
    pool: &SqlitePool,
    session_id: &str,
) -> anyhow::Result<Option<EventsSnapshot>> {
    let source = match transcript_source_for_session(pool, session_id).await? {
        Some(source) => source,
        None => return Ok(None),
    };
    // Atomic read: events + byte_offset + line_count all come from the
    // same read so (a) the SSE tail cannot skip lines that appended
    // between two separate reads, and (b) next_line stays aligned with
    // the source file even when empty lines are present (both flagged
    // on PR #975 review).
    let (events, byte_offset, line_count) = parse_events_with_size(&source);
    let next_line = line_count.saturating_add(1);
    let is_running = is_session_running(&source).await;
    Ok(Some(EventsSnapshot {
        events,
        is_running,
        source: Some(source),
        byte_offset,
        next_line,
    }))
}

/// Resolve the absolute path of the session's underlying JSONL file on disk,
/// preferring the Mando-owned stream under `~/.mando/state/cc-streams/` and
/// falling back to the CC-native `~/.claude/projects/` layout. Returns `None`
/// when neither file exists.
#[tracing::instrument(skip_all)]
pub async fn load_jsonl_path(
    pool: &SqlitePool,
    session_id: &str,
) -> anyhow::Result<Option<String>> {
    if is_codex_session(pool, session_id).await? {
        if let Some(stream) = codex_jsonl_path_for_session(session_id).await? {
            return Ok(Some(stream.to_string_lossy().into_owned()));
        }
        if let Some(stream) = codex_derived_stream_path_for_session(session_id).await? {
            return Ok(Some(stream.to_string_lossy().into_owned()));
        }
    }

    if let Some(stream) = stream_path_for_session(session_id).await? {
        return Ok(Some(stream.to_string_lossy().into_owned()));
    }

    let cwd = crate::io::queries::session_cwd(pool, session_id).await?;
    if let Some(path) = find_cc_transcript_path(session_id, cwd.as_deref()).await? {
        return Ok(Some(path.to_string_lossy().into_owned()));
    }

    Ok(None)
}

pub fn parse_events_from_source(
    source: &TranscriptSource,
    byte_offset: u64,
    starting_line_number: u32,
) -> (Vec<api_types::TranscriptEvent>, u64) {
    match source.format {
        TranscriptFormat::ClaudeCode => {
            global_claude::parse_events_from_offset(&source.path, byte_offset, starting_line_number)
        }
        TranscriptFormat::CodexAppServer => {
            crate::runtime::codex_transcript::parse_events_from_offset(
                &source.path,
                byte_offset,
                starting_line_number,
            )
        }
    }
}

pub fn is_source_finished(session_id: &str, source: &TranscriptSource) -> bool {
    source
        .meta_path
        .as_ref()
        .map(|path| global_claude::is_stream_meta_finished_at(path))
        .unwrap_or_else(|| match source.format {
            TranscriptFormat::ClaudeCode => global_claude::is_session_finished(session_id),
            TranscriptFormat::CodexAppServer => false,
        })
}

fn parse_events_with_size(
    source: &TranscriptSource,
) -> (Vec<api_types::TranscriptEvent>, u64, u32) {
    match source.format {
        TranscriptFormat::ClaudeCode => global_claude::parse_events_with_size(&source.path),
        TranscriptFormat::CodexAppServer => {
            crate::runtime::codex_transcript::parse_events_with_size(&source.path)
        }
    }
}

#[tracing::instrument(skip_all)]
pub async fn load_messages(
    pool: &SqlitePool,
    session_id: &str,
    limit: Option<usize>,
    offset: usize,
) -> anyhow::Result<Option<Vec<global_claude::TranscriptMessage>>> {
    let Some(stream) = claude_format_stream_path_for_session(pool, session_id).await? else {
        return Ok(None);
    };
    Ok(Some(global_claude::parse_messages(&stream, limit, offset)))
}

#[tracing::instrument(skip_all)]
pub async fn load_tool_usage(
    pool: &SqlitePool,
    session_id: &str,
) -> anyhow::Result<Option<Vec<global_claude::ToolUsageSummary>>> {
    let Some(stream) = claude_format_stream_path_for_session(pool, session_id).await? else {
        return Ok(None);
    };
    Ok(Some(global_claude::tool_usage(&stream)))
}

#[tracing::instrument(skip_all)]
pub async fn load_session_cost(
    pool: &SqlitePool,
    session_id: &str,
) -> anyhow::Result<Option<global_claude::SessionCost>> {
    let Some(stream) = claude_format_stream_path_for_session(pool, session_id).await? else {
        return Ok(None);
    };
    Ok(Some(global_claude::session_cost(&stream)))
}

#[tracing::instrument(skip_all)]
pub async fn load_session_stream(
    pool: &SqlitePool,
    session_id: &str,
    types: Option<Vec<String>>,
) -> anyhow::Result<Option<String>> {
    let allowed = types
        .map(|items| {
            items
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<HashSet<_>>()
        })
        .filter(|items| !items.is_empty());
    let stream = if allowed.is_some() && is_codex_session(pool, session_id).await? {
        match codex_derived_stream_path_for_session(session_id).await? {
            Some(path) => path,
            None => match codex_jsonl_path_for_session(session_id).await? {
                Some(path) => path,
                None => return Ok(None),
            },
        }
    } else {
        let Some(stream) = load_jsonl_path(pool, session_id).await? else {
            return Ok(None);
        };
        PathBuf::from(stream)
    };

    let content = tokio::fs::read_to_string(&stream)
        .await
        .with_context(|| format!("failed to read session stream for {session_id}"))?;

    let mut dropped = 0usize;
    let filtered = content
        .lines()
        .filter(|line| {
            let Some(ref allowed) = allowed else {
                return true;
            };
            let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
                dropped += 1;
                return false;
            };
            val["type"]
                .as_str()
                .is_some_and(|kind| allowed.contains(kind))
        })
        .collect::<Vec<_>>()
        .join("\n");

    if dropped > 0 {
        tracing::warn!(
            module = "sessions",
            session_id = %session_id,
            dropped,
            "stream filter skipped malformed JSONL lines",
        );
    }

    Ok(Some(filtered))
}

async fn stream_path_for_session(session_id: &str) -> anyhow::Result<Option<PathBuf>> {
    let stream = global_infra::paths::stream_path_for_session(session_id);
    if path_exists(&stream).await? {
        Ok(Some(stream))
    } else {
        Ok(None)
    }
}

async fn claude_format_stream_path_for_session(
    pool: &SqlitePool,
    session_id: &str,
) -> anyhow::Result<Option<PathBuf>> {
    if is_codex_session(pool, session_id).await? {
        if let Some(stream) = codex_derived_stream_path_for_session(session_id).await? {
            return Ok(Some(stream));
        }
    }
    stream_path_for_session(session_id).await
}

async fn codex_jsonl_path_for_session(session_id: &str) -> anyhow::Result<Option<PathBuf>> {
    let stream = global_infra::paths::codex_session_jsonl_path(session_id);
    if path_exists(&stream).await? {
        Ok(Some(stream))
    } else {
        Ok(None)
    }
}

async fn transcript_source_for_session(
    pool: &SqlitePool,
    session_id: &str,
) -> anyhow::Result<Option<TranscriptSource>> {
    if is_codex_session(pool, session_id).await? {
        if let Some(path) = codex_jsonl_path_for_session(session_id).await? {
            return Ok(Some(TranscriptSource {
                path,
                format: TranscriptFormat::CodexAppServer,
                meta_path: Some(
                    global_infra::paths::codex_derived_stream_meta_path_for_session(session_id),
                ),
            }));
        }
        if let Some(path) = codex_derived_stream_path_for_session(session_id).await? {
            return Ok(Some(TranscriptSource {
                path,
                format: TranscriptFormat::ClaudeCode,
                meta_path: Some(
                    global_infra::paths::codex_derived_stream_meta_path_for_session(session_id),
                ),
            }));
        }
    }

    // Prefer the Mando-owned Claude Code `cc-streams/` path; otherwise fall
    // back to the CC-native `~/.claude/projects/` layout so sessions started
    // outside Mando or recovered after cache eviction still surface.
    if let Some(path) = stream_path_for_session(session_id).await? {
        return Ok(Some(TranscriptSource {
            path,
            format: TranscriptFormat::ClaudeCode,
            meta_path: Some(global_infra::paths::stream_meta_path_for_session(
                session_id,
            )),
        }));
    }

    if is_codex_session(pool, session_id).await? {
        return Ok(None);
    }

    let cwd = crate::io::queries::session_cwd(pool, session_id).await?;
    if let Some(path) = find_cc_transcript_path(session_id, cwd.as_deref()).await? {
        return Ok(Some(TranscriptSource {
            path,
            format: TranscriptFormat::ClaudeCode,
            meta_path: Some(global_infra::paths::stream_meta_path_for_session(
                session_id,
            )),
        }));
    }

    Ok(None)
}

async fn is_codex_session(pool: &SqlitePool, session_id: &str) -> anyhow::Result<bool> {
    Ok(crate::io::queries::session_by_id(pool, session_id)
        .await?
        .is_some_and(|session| session.provider == global_types::TaskProvider::Codex))
}

async fn codex_derived_stream_path_for_session(
    session_id: &str,
) -> anyhow::Result<Option<PathBuf>> {
    let stream = global_infra::paths::codex_derived_stream_path_for_session(session_id);
    if path_exists(&stream).await? {
        Ok(Some(stream))
    } else {
        Ok(None)
    }
}

async fn path_exists(path: &Path) -> anyhow::Result<bool> {
    tokio::fs::try_exists(path)
        .await
        .with_context(|| format!("stat session stream {}", path.display()))
}

enum StreamMeta {
    Found(serde_json::Value),
    Corrupt,
    Missing,
}

async fn read_stream_meta_at(meta_path: &Path) -> StreamMeta {
    let content = match tokio::fs::read_to_string(meta_path).await {
        Ok(content) => content,
        Err(_) => return StreamMeta::Missing,
    };
    match serde_json::from_str::<serde_json::Value>(&content) {
        Ok(value) => StreamMeta::Found(value),
        Err(err) => {
            tracing::warn!(
                module = "sessions",
                path = %meta_path.display(),
                error = %err,
                "stream meta corrupt",
            );
            StreamMeta::Corrupt
        }
    }
}

async fn is_session_running(source: &TranscriptSource) -> bool {
    let Some(meta_path) = source.meta_path.as_ref() else {
        return false;
    };
    match read_stream_meta_at(meta_path).await {
        StreamMeta::Found(value) => value["status"].as_str() == Some("running"),
        StreamMeta::Corrupt => true,
        StreamMeta::Missing => false,
    }
}
async fn find_cc_transcript_path(
    session_id: &str,
    cwd: Option<&str>,
) -> anyhow::Result<Option<PathBuf>> {
    let Some(projects_dir) = cc_projects_dir() else {
        return Ok(None);
    };
    let target = format!("{session_id}.jsonl");
    let effective_cwd = resolve_effective_cwd(session_id, cwd).await;

    if let Some(path) = cwd_candidate(&projects_dir, effective_cwd.as_deref(), &target) {
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Some(path));
        }
    }

    if !tokio::fs::try_exists(&projects_dir).await.unwrap_or(false) {
        return Ok(None);
    }

    let mut entries = tokio::fs::read_dir(&projects_dir)
        .await
        .with_context(|| format!("failed to read {}", projects_dir.display()))?;
    while let Some(entry) = entries.next_entry().await? {
        let candidate = entry.path().join(&target);
        if tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
            return Ok(Some(candidate));
        }
    }

    Ok(None)
}

fn cc_projects_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".claude").join("projects"))
}

async fn resolve_effective_cwd(session_id: &str, cwd: Option<&str>) -> Option<String> {
    match cwd.map(String::from) {
        Some(cwd) => Some(cwd),
        None => lookup_cwd_from_meta(session_id).await,
    }
}

fn cwd_candidate(projects_dir: &Path, cwd: Option<&str>, target: &str) -> Option<PathBuf> {
    let cwd = cwd?;
    if cwd.is_empty() {
        return None;
    }
    let sanitized = cwd.replace('/', "-");
    Some(projects_dir.join(sanitized).join(target))
}

async fn lookup_cwd_from_meta(session_id: &str) -> Option<String> {
    let meta_path = global_infra::paths::stream_meta_path_for_session(session_id);
    let value = match read_stream_meta_at(&meta_path).await {
        StreamMeta::Found(value) => value,
        StreamMeta::Corrupt | StreamMeta::Missing => return None,
    };
    value["cwd"]
        .as_str()
        .filter(|cwd| !cwd.is_empty())
        .map(String::from)
}
