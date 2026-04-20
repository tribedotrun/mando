use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Context;
use sqlx::SqlitePool;

#[tracing::instrument(skip_all)]
pub async fn load_transcript_markdown(
    pool: &SqlitePool,
    session_id: &str,
) -> anyhow::Result<Option<String>> {
    let cache_dir = global_infra::paths::state_dir().join("transcripts");
    let md_path = cache_dir.join(format!("{session_id}.md"));
    if let Ok(content) = tokio::fs::read_to_string(&md_path).await {
        return Ok(Some(content));
    }

    let cwd = crate::io::queries::session_cwd(pool, session_id).await?;
    let Some(jsonl) = find_cc_transcript(session_id, cwd.as_deref()).await? else {
        return Ok(None);
    };

    let markdown = crate::io::transcript::jsonl_to_markdown(&jsonl);
    if !is_session_running(session_id).await {
        if let Err(err) = tokio::fs::create_dir_all(&cache_dir).await {
            tracing::warn!(module = "sessions", error = %err, "failed to create transcript cache dir");
        } else if let Err(err) =
            tokio::fs::write(cache_dir.join(format!("{session_id}.md")), &markdown).await
        {
            tracing::warn!(
                module = "sessions",
                session_id = %session_id,
                error = %err,
                "failed to cache transcript",
            );
        }
    }

    Ok(Some(markdown))
}

#[tracing::instrument(skip_all)]
pub async fn load_messages(
    session_id: &str,
    limit: Option<usize>,
    offset: usize,
) -> anyhow::Result<Option<Vec<global_claude::TranscriptMessage>>> {
    let Some(stream) = stream_path_for_session(session_id).await? else {
        return Ok(None);
    };
    Ok(Some(global_claude::parse_messages(&stream, limit, offset)))
}

#[tracing::instrument(skip_all)]
pub async fn load_tool_usage(
    session_id: &str,
) -> anyhow::Result<Option<Vec<global_claude::ToolUsageSummary>>> {
    let Some(stream) = stream_path_for_session(session_id).await? else {
        return Ok(None);
    };
    Ok(Some(global_claude::tool_usage(&stream)))
}

#[tracing::instrument(skip_all)]
pub async fn load_session_cost(
    session_id: &str,
) -> anyhow::Result<Option<global_claude::SessionCost>> {
    let Some(stream) = stream_path_for_session(session_id).await? else {
        return Ok(None);
    };
    Ok(Some(global_claude::session_cost(&stream)))
}

#[tracing::instrument(skip_all)]
pub async fn load_session_stream(
    session_id: &str,
    types: Option<Vec<String>>,
) -> anyhow::Result<Option<String>> {
    let Some(stream) = stream_path_for_session(session_id).await? else {
        return Ok(None);
    };

    let content = tokio::fs::read_to_string(&stream)
        .await
        .with_context(|| format!("failed to read session stream for {session_id}"))?;
    let allowed = types
        .map(|items| {
            items
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<HashSet<_>>()
        })
        .filter(|items| !items.is_empty());

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
    if tokio::fs::try_exists(&stream).await.unwrap_or(false) {
        Ok(Some(stream))
    } else {
        Ok(None)
    }
}

enum StreamMeta {
    Found(serde_json::Value),
    Corrupt,
    Missing,
}

async fn read_stream_meta(session_id: &str) -> StreamMeta {
    let meta_path = global_infra::paths::state_dir()
        .join("cc-streams")
        .join(format!("{session_id}.meta.json"));
    let content = match tokio::fs::read_to_string(&meta_path).await {
        Ok(content) => content,
        Err(_) => return StreamMeta::Missing,
    };
    match serde_json::from_str::<serde_json::Value>(&content) {
        Ok(value) => StreamMeta::Found(value),
        Err(err) => {
            tracing::warn!(
                module = "sessions",
                session_id = %session_id,
                error = %err,
                "stream meta corrupt",
            );
            StreamMeta::Corrupt
        }
    }
}

async fn is_session_running(session_id: &str) -> bool {
    match read_stream_meta(session_id).await {
        StreamMeta::Found(value) => value["status"].as_str() == Some("running"),
        StreamMeta::Corrupt => true,
        StreamMeta::Missing => false,
    }
}

async fn find_cc_transcript(session_id: &str, cwd: Option<&str>) -> anyhow::Result<Option<String>> {
    let Some(home) = std::env::var("HOME").ok() else {
        return Ok(None);
    };
    let projects_dir = PathBuf::from(home).join(".claude").join("projects");
    let target = format!("{session_id}.jsonl");

    let effective_cwd = match cwd.map(String::from) {
        Some(cwd) => Some(cwd),
        None => lookup_cwd_from_meta(session_id).await,
    };
    if let Some(cwd) = effective_cwd {
        if !cwd.is_empty() {
            let sanitized = cwd.replace('/', "-");
            let path = projects_dir.join(&sanitized).join(&target);
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                return Ok(Some(content));
            }
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
        if let Ok(content) = tokio::fs::read_to_string(&candidate).await {
            return Ok(Some(content));
        }
    }

    Ok(None)
}

async fn lookup_cwd_from_meta(session_id: &str) -> Option<String> {
    let value = match read_stream_meta(session_id).await {
        StreamMeta::Found(value) => value,
        StreamMeta::Corrupt | StreamMeta::Missing => return None,
    };
    value["cwd"]
        .as_str()
        .filter(|cwd| !cwd.is_empty())
        .map(String::from)
}
