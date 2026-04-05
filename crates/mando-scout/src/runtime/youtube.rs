//! YouTube transcript extraction.

use anyhow::{Context, Result};

async fn cleanup_tmp_dir(dir: &std::path::Path) {
    if let Err(e) = tokio::fs::remove_dir_all(dir).await {
        tracing::warn!(
            path = %dir.display(),
            error = %e,
            "yt-dlp tmp dir cleanup failed (leaked files)"
        );
    }
}

/// Extract transcript text from a YouTube video URL.
///
/// Uses `yt-dlp --write-sub --skip-download` to fetch subtitles, then
/// parses the VTT/SRT content into plain text.
pub async fn extract_youtube_transcript(url: &str) -> Result<String> {
    let tmp_dir = std::env::temp_dir().join(format!("mando-yt-{}", mando_uuid::Uuid::v4()));
    tokio::fs::create_dir_all(&tmp_dir).await?;

    // Ensure yt-dlp binary is available (downloads on first use).
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
        Ok(out) if out.status.success() => {
            // Find the subtitle file.
            let mut transcript = String::new();
            let mut entries = tokio::fs::read_dir(&tmp_dir)
                .await
                .context("failed to read yt-dlp output directory")?;
            {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("vtt") {
                        let content = tokio::fs::read_to_string(&path).await?;
                        transcript = parse_vtt(&content);
                        break;
                    }
                }
            }

            // Cleanup.
            cleanup_tmp_dir(&tmp_dir).await;

            if transcript.is_empty() {
                anyhow::bail!("no subtitles found for {url}");
            }
            Ok(transcript)
        }
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
}

/// Parse WebVTT subtitle content into plain text.
fn parse_vtt(vtt: &str) -> String {
    let mut lines = Vec::new();
    let mut prev_line = String::new();

    for line in vtt.lines() {
        let trimmed = line.trim();
        // Skip headers, timestamps, and empty lines.
        if trimmed.is_empty()
            || trimmed.starts_with("WEBVTT")
            || trimmed.starts_with("Kind:")
            || trimmed.starts_with("Language:")
            || trimmed.contains("-->")
            || trimmed.parse::<u32>().is_ok()
        {
            continue;
        }

        // Strip VTT formatting tags.
        let clean = strip_vtt_tags(trimmed);
        if clean.is_empty() || clean == prev_line {
            continue;
        }

        prev_line = clean.clone();
        lines.push(clean);
    }

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
}
