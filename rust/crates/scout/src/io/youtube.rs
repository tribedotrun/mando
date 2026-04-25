//! YouTube transcript + metadata extraction.
//!
//! Single `yt-dlp` invocation produces two sidecar files in a temp dir:
//! `<id>.en.vtt` (auto-subtitles) and `<id>.info.json` (title, upload date,
//! channel). Both are read deterministically; no LLM involved.

use anyhow::{Context, Result};
use serde::Deserialize;

use super::metadata_probe;

/// Result of a YouTube extraction pass.
#[derive(Debug)]
pub struct YoutubeResult {
    pub transcript: String,
    pub title: Option<String>,
    /// Publication date normalized to `YYYY-MM-DD`. Sourced from
    /// `release_date` (the public go-live date for Premieres, equal to
    /// `upload_date` for normal videos) or `upload_date` as fallback.
    /// `None` when yt-dlp returned neither.
    pub publish_date: Option<String>,
}

/// Subset of yt-dlp's `.info.json` we care about. yt-dlp emits many more
/// fields (formats, thumbnails, captions URLs) — we deliberately ignore
/// them so new yt-dlp fields don't destabilize our boundary.
#[derive(Debug, Deserialize)]
struct YtDlpInfo {
    title: Option<String>,
    upload_date: Option<String>,
    release_date: Option<String>,
}

async fn cleanup_tmp_dir(dir: &std::path::Path) {
    if let Err(e) = tokio::fs::remove_dir_all(dir).await {
        tracing::warn!(
            module = "scout-io-youtube", path = %dir.display(),
            error = %e,
            "yt-dlp tmp dir cleanup failed (leaked files)"
        );
    }
}

/// Extract transcript + metadata from a YouTube video URL.
///
/// Runs `yt-dlp --write-auto-sub --write-info-json --skip-download`. The
/// info.json sidecar is parsed for title and publish date; the VTT sidecar
/// is parsed for transcript text. A missing info.json or missing fields are
/// not fatal — the function still returns the transcript.
pub async fn extract_youtube_transcript(url: &str) -> Result<YoutubeResult> {
    let tmp_dir = std::env::temp_dir().join(format!("mando-yt-{}", global_infra::uuid::Uuid::v4()));
    tokio::fs::create_dir_all(&tmp_dir).await?;

    let yt_dlp_bin = crate::io::yt_dlp::ensure_yt_dlp().await?;

    // Explicit stdio: daemon context may have invalid FDs 0/1/2 (e.g. revoked PTY),
    // which causes yt-dlp's embedded Python runtime to crash at init_sys_streams with EBADF.
    let output = tokio::process::Command::new(&yt_dlp_bin)
        .args([
            "--write-auto-sub",
            "--sub-lang",
            "en",
            "--skip-download",
            "--sub-format",
            "vtt",
            "--write-info-json",
            "-o",
            tmp_dir.join("%(id)s").to_str().unwrap_or("video"),
            url,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            cleanup_tmp_dir(&tmp_dir).await;
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("yt-dlp failed: {stderr}");
        }
        Err(e) => {
            cleanup_tmp_dir(&tmp_dir).await;
            anyhow::bail!("yt-dlp exec failed ({}): {e}", yt_dlp_bin.display());
        }
    }

    let sidecars = read_sidecars(&tmp_dir).await;
    cleanup_tmp_dir(&tmp_dir).await;

    let Sidecars {
        transcript,
        info_json,
    } = sidecars?;

    if transcript.is_empty() {
        anyhow::bail!("no subtitles found for {url}");
    }

    let (title, publish_date) = info_json
        .and_then(|raw| match serde_json::from_str::<YtDlpInfo>(&raw) {
            Ok(info) => {
                // Premieres set `release_date` to the public go-live date and
                // `upload_date` to the earlier ingest time; normal videos set
                // them equal or leave `release_date` null. Prefer
                // release_date so Premieres date-stamp to when viewers saw
                // them, not when the file was uploaded for scheduling.
                let date = info
                    .release_date
                    .as_deref()
                    .or(info.upload_date.as_deref())
                    .and_then(metadata_probe::normalize_date);
                Some((info.title, date))
            }
            Err(e) => {
                tracing::warn!(
                    module = "scout-io-youtube",
                    url,
                    error = %e,
                    "yt-dlp info.json parse failed — date/title unavailable"
                );
                None
            }
        })
        .unwrap_or((None, None));

    Ok(YoutubeResult {
        transcript,
        title,
        publish_date,
    })
}

struct Sidecars {
    transcript: String,
    info_json: Option<String>,
}

async fn read_sidecars(dir: &std::path::Path) -> Result<Sidecars> {
    let mut transcript = String::new();
    let mut info_json: Option<String> = None;
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .context("failed to read yt-dlp output directory")?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        match path.extension().and_then(|e| e.to_str()) {
            Some("vtt") if transcript.is_empty() => {
                let content = tokio::fs::read_to_string(&path).await?;
                transcript = parse_vtt(&content);
            }
            Some("json") if info_json.is_none() => {
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".info.json"))
                {
                    info_json = Some(tokio::fs::read_to_string(&path).await?);
                }
            }
            _ => {}
        }
    }
    Ok(Sidecars {
        transcript,
        info_json,
    })
}

/// Parse WebVTT subtitle content into plain text.
fn parse_vtt(vtt: &str) -> String {
    let mut lines: Vec<String> = vtt
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("WEBVTT")
                || trimmed.starts_with("Kind:")
                || trimmed.starts_with("Language:")
                || trimmed.contains("-->")
                || trimmed.parse::<u32>().is_ok()
            {
                return None;
            }
            let clean = strip_vtt_tags(trimmed);
            if clean.is_empty() {
                None
            } else {
                Some(clean)
            }
        })
        .collect();

    lines.dedup();
    lines.join(" ")
}

fn strip_vtt_tags(text: &str) -> String {
    crate::io::strip_html_tags(text).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vtt_basic() {
        let vtt = "WEBVTT\nKind: captions\nLanguage: en\n\n\
                    00:00:01.000 --> 00:00:05.000\n\
                    Hello world\n\n\
                    00:00:05.000 --> 00:00:10.000\n\
                    This is a test\n";
        let result = parse_vtt(vtt);
        assert!(result.contains("Hello world"));
        assert!(result.contains("This is a test"));
    }

    #[test]
    fn strip_tags() {
        assert_eq!(strip_vtt_tags("<c>Hello</c> world"), "Hello world");
    }

    /// Resolve publish date the same way production does:
    /// release_date first, upload_date fallback.
    fn resolve_publish(info: &YtDlpInfo) -> Option<String> {
        info.release_date
            .as_deref()
            .or(info.upload_date.as_deref())
            .and_then(metadata_probe::normalize_date)
    }

    #[test]
    fn info_json_parses_upload_date() {
        let raw = r#"{"id":"abc","title":"Demo","upload_date":"20260405"}"#;
        let info: YtDlpInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.title.as_deref(), Some("Demo"));
        assert_eq!(resolve_publish(&info).as_deref(), Some("2026-04-05"));
    }

    #[test]
    fn info_json_upload_date_fallback_when_release_null() {
        let raw =
            r#"{"id":"abc","title":"Normal Upload","release_date":null,"upload_date":"20251017"}"#;
        let info: YtDlpInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(resolve_publish(&info).as_deref(), Some("2025-10-17"));
    }

    #[test]
    fn info_json_premiere_prefers_release_over_upload() {
        // Premieres have release_date = public go-live, upload_date = earlier
        // ingest. The earlier upload_date must not win.
        let raw =
            r#"{"id":"abc","title":"Premiere","release_date":"20260405","upload_date":"20260320"}"#;
        let info: YtDlpInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(resolve_publish(&info).as_deref(), Some("2026-04-05"));
    }

    #[test]
    fn info_json_tolerates_unknown_fields() {
        let raw = r#"{
            "id":"abc","title":"T","upload_date":"20260101",
            "formats":[{"foo":"bar"}],"thumbnails":[],"extra":"ignored"
        }"#;
        let info: YtDlpInfo = serde_json::from_str(raw).unwrap();
        assert_eq!(info.title.as_deref(), Some("T"));
    }
}
