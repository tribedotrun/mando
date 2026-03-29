//! Download PR evidence images and extract video frames.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Media extensions we recognize as evidence.
const IMAGE_EXTS: &[&str] = &[".png", ".jpg", ".jpeg", ".gif"];
const VIDEO_EXTS: &[&str] = &[".mp4", ".mov", ".webm"];

/// Max evidence URLs to process per PR (keeps review fast).
const MAX_EVIDENCE_URLS: usize = 3;

/// Extract evidence image/video URLs from a PR body.
///
/// Finds markdown images `![...](url)` and raw `https://` URLs ending in
/// known media extensions. Caps at `MAX_EVIDENCE_URLS`.
pub(crate) fn extract_evidence_urls(pr_body: &str) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    let all_exts: Vec<&str> = IMAGE_EXTS
        .iter()
        .chain(VIDEO_EXTS.iter())
        .copied()
        .collect();

    for line in pr_body.lines() {
        // Markdown images: ![alt](url)
        let mut rest = line;
        while let Some(start) = rest.find("![") {
            rest = &rest[start + 2..];
            if let Some(paren_start) = rest.find("](") {
                let url_start = paren_start + 2;
                if let Some(paren_end) = rest[url_start..].find(')') {
                    let url = rest[url_start..url_start + paren_end].trim();
                    if url.starts_with("http") {
                        urls.push(url.to_string());
                    }
                    rest = &rest[url_start + paren_end..];
                }
            }
        }

        // Raw URLs with media extensions (strip query params for ext check)
        for word in line.split_whitespace() {
            let cleaned = word.trim_matches(|c: char| c == '(' || c == ')' || c == '<' || c == '>');
            if cleaned.starts_with("http") {
                let path_lower = url_without_query(cleaned).to_lowercase();
                if all_exts.iter().any(|ext| path_lower.ends_with(ext))
                    && !urls.contains(&cleaned.to_string())
                {
                    urls.push(cleaned.to_string());
                }
            }
        }

        if urls.len() >= MAX_EVIDENCE_URLS {
            break;
        }
    }

    urls.truncate(MAX_EVIDENCE_URLS);
    urls
}

/// Strip query parameters from a URL for extension checking.
fn url_without_query(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

/// Returns true if the URL points to a video (not an image).
pub(crate) fn is_video_url(url: &str) -> bool {
    let lower = url_without_query(url).to_lowercase();
    VIDEO_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// Extract file extension from URL (without query params), defaulting to "png".
fn url_extension(url: &str) -> &str {
    let path = url_without_query(url);
    if let Some(dot) = path.rfind('.') {
        let ext = &path[dot + 1..];
        if ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return ext;
        }
    }
    "png"
}

/// Download an image from a URL to the images directory.
pub(crate) async fn download_image(
    url: &str,
    images_dir: &Path,
    filename: &str,
) -> Result<PathBuf> {
    tokio::fs::create_dir_all(images_dir).await?;
    let dest = images_dir.join(filename);

    let resp = reqwest::get(url).await?.error_for_status()?;
    let bytes = resp.bytes().await?;
    tokio::fs::write(&dest, &bytes).await?;

    Ok(dest)
}

/// Extract a frame from a video file using ffmpeg.
pub(crate) async fn extract_frame(
    video_path: &Path,
    output_path: &Path,
    timestamp: &str,
) -> Result<()> {
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(video_path)
        .args(["-ss", timestamp, "-frames:v", "1"])
        .arg(output_path)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg extract frame failed: {}", stderr);
    }
    Ok(())
}

/// Download evidence images/videos and extract key frames.
///
/// Returns a list of local file paths that captain can read.
/// Cleans up old evidence before downloading fresh copies.
pub(crate) async fn download_evidence(pr_body: &str, work_dir: &Path) -> Vec<PathBuf> {
    let urls = extract_evidence_urls(pr_body);
    if urls.is_empty() {
        return Vec::new();
    }

    let images_dir = work_dir.join("evidence");
    // Clean stale evidence from prior ticks.
    tokio::fs::remove_dir_all(&images_dir).await.ok();

    let mut paths = Vec::new();
    let mut download_failures = 0usize;

    for (i, url) in urls.iter().enumerate() {
        let ext = url_extension(url);
        let filename = format!("evidence_{}.{}", i, ext);

        match download_image(url, &images_dir, &filename).await {
            Ok(path) => {
                if is_video_url(url) {
                    let mut frames_ok = 0usize;
                    for (j, ts) in ["00:00:01", "00:00:05", "00:00:10"].iter().enumerate() {
                        let frame_name = format!("evidence_{}_frame_{}.png", i, j);
                        let frame_path = images_dir.join(&frame_name);
                        match extract_frame(&path, &frame_path, ts).await {
                            Ok(()) => {
                                frames_ok += 1;
                                paths.push(frame_path);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    module = "evidence",
                                    url = %url,
                                    timestamp = ts,
                                    error = %e,
                                    "ffmpeg frame extraction failed"
                                );
                            }
                        }
                    }
                    if frames_ok == 0 {
                        tracing::error!(
                            module = "evidence",
                            url = %url,
                            "all frame extractions failed — ffmpeg may be missing or video corrupt"
                        );
                    }
                } else {
                    paths.push(path);
                }
            }
            Err(e) => {
                download_failures += 1;
                tracing::warn!(
                    module = "evidence",
                    url = %url,
                    error = %e,
                    "failed to download evidence"
                );
            }
        }
    }

    if download_failures == urls.len() && !urls.is_empty() {
        tracing::error!(
            module = "evidence",
            attempted = urls.len(),
            "all evidence downloads failed — review will proceed without visual inspection"
        );
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_markdown_images() {
        let body = "### After\n![screenshot](https://example.com/img.png)\nSome text";
        let urls = extract_evidence_urls(body);
        assert_eq!(urls, vec!["https://example.com/img.png"]);
    }

    #[test]
    fn extract_raw_urls() {
        let body = "### After\nhttps://storage.example.com/evidence.gif\nText";
        let urls = extract_evidence_urls(body);
        assert_eq!(urls, vec!["https://storage.example.com/evidence.gif"]);
    }

    #[test]
    fn caps_at_max() {
        let body = "![a](https://a.com/1.png)\n![b](https://b.com/2.png)\n![c](https://c.com/3.png)\n![d](https://d.com/4.png)";
        let urls = extract_evidence_urls(body);
        assert_eq!(urls.len(), 3);
    }

    #[test]
    fn no_evidence_empty() {
        let urls = extract_evidence_urls("## Summary\nJust text");
        assert!(urls.is_empty());
    }

    #[test]
    fn deduplicates() {
        let body = "![a](https://a.com/1.png) https://a.com/1.png";
        let urls = extract_evidence_urls(body);
        assert_eq!(urls.len(), 1);
    }

    #[test]
    fn video_detection() {
        assert!(is_video_url("https://example.com/demo.mp4"));
        assert!(is_video_url("https://example.com/demo.webm"));
        assert!(!is_video_url("https://example.com/demo.png"));
    }

    #[test]
    fn url_with_query_params() {
        let body =
            "### After\nhttps://user-images.githubusercontent.com/123/abc.png?X-Amz-Algorithm=foo";
        let urls = extract_evidence_urls(body);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].contains("abc.png"));
    }

    #[test]
    fn url_extension_extracts_correct() {
        assert_eq!(url_extension("https://example.com/fix.png"), "png");
        assert_eq!(url_extension("https://example.com/demo.mp4"), "mp4");
        assert_eq!(
            url_extension("https://example.com/img.jpg?token=abc"),
            "jpg"
        );
        assert_eq!(url_extension("https://example.com/no-extension"), "png");
    }
}
