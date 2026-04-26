//! Motion check for screen-recording evidence.
//!
//! Mirrors the JS engine at `devtools/mando-dev/obs/motion_check.mjs`. Both
//! must produce the same verdict for the same input; sample three points in
//! the clip (first / middle / last), threshold per-pixel luma differences,
//! reject when no sampled pair has at least `MIN_CHANGED_FRACTION` (0.1%) of
//! pixels above `LUMA_DIFF_THRESHOLD` (5/255). Calibrated against PR #977's
//! degenerate clip (changed_fraction = 0.0008%).
//!
//! This is the authoritative gate at `mando todo evidence` ingest, so a
//! recording from any source (mando-dev recorder, screencapture, OBS,
//! MediaRecorder, ScreenCaptureKit) gets validated here regardless of how it
//! was produced.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Pixels are "changed" between two frames if their 8-bit luma differs by
/// more than this many levels. Mirrors `LUMA_DIFF_THRESHOLD` in the JS engine.
pub const LUMA_DIFF_THRESHOLD: u8 = 5;

/// Reject recordings unless at least this fraction of pixels changed in at
/// least one sampled frame pair. PR #977's empirical bad case is 0.0008%, so
/// 0.1% leaves ~10x headroom. Mirrors `MIN_CHANGED_FRACTION` in the JS engine.
pub const MIN_CHANGED_FRACTION: f64 = 0.001;

/// Outcome of the motion check.
///
/// `mean_diff` and per-pair indices are part of the public surface so future
/// callers (e.g. the `mando-dev evidence verify` audit) can render a richer
/// table than `handle_evidence` needs today.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MotionVerdict {
    pub verdict: Verdict,
    pub changed_fraction: f64,
    pub mean_diff: f64,
    pub sampled_pairs: Vec<PairStats>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Degenerate,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PairStats {
    pub a_index: usize,
    pub b_index: usize,
    pub changed_fraction: f64,
    pub mean_diff: f64,
}

/// Inspect a video file (mp4/mov/webm) and decide whether the clip is
/// visually static. Extracts three sample frames via ffmpeg, then compares
/// each pair via signalstats.
pub fn check_video(video_path: &Path) -> Result<MotionVerdict, MotionCheckError> {
    let temp = tempdir()?;
    let frames = extract_sample_frames(video_path, temp.path())?;
    if frames.len() < 2 {
        return Ok(MotionVerdict {
            verdict: Verdict::Degenerate,
            changed_fraction: 0.0,
            mean_diff: 0.0,
            sampled_pairs: vec![],
            reason: format!(
                "recording has {} sample frame(s); need at least 2 to detect motion",
                frames.len()
            ),
        });
    }

    let pair_indices = pick_pair_indices(frames.len());
    let mut pair_stats: Vec<PairStats> = Vec::with_capacity(pair_indices.len());
    for (a, b) in &pair_indices {
        let stats = diff_stats(&frames[*a], &frames[*b])?;
        pair_stats.push(PairStats {
            a_index: *a,
            b_index: *b,
            changed_fraction: stats.changed_fraction,
            mean_diff: stats.mean_diff,
        });
    }

    let max_changed = pair_stats
        .iter()
        .map(|p| p.changed_fraction)
        .fold(0.0_f64, f64::max);
    let max_mean = pair_stats
        .iter()
        .map(|p| p.mean_diff)
        .fold(0.0_f64, f64::max);

    if max_changed < MIN_CHANGED_FRACTION {
        let reason = format!(
            "recording rejected: changed_fraction={:.4}% across {} sampled frame pair(s) \
             (floor {:.1}%). Frame range was visually static; re-record so the action \
             being demonstrated happens during the recording window.",
            max_changed * 100.0,
            pair_stats.len(),
            MIN_CHANGED_FRACTION * 100.0,
        );
        return Ok(MotionVerdict {
            verdict: Verdict::Degenerate,
            changed_fraction: max_changed,
            mean_diff: max_mean,
            sampled_pairs: pair_stats,
            reason,
        });
    }

    let reason = format!(
        "recording ok: max changed_fraction={:.4}% over {} sampled pair(s)",
        max_changed * 100.0,
        pair_stats.len()
    );
    Ok(MotionVerdict {
        verdict: Verdict::Ok,
        changed_fraction: max_changed,
        mean_diff: max_mean,
        sampled_pairs: pair_stats,
        reason,
    })
}

/// Extract first / middle / last frames from a video into a temp directory.
/// Returns the resulting PNG paths in order.
fn extract_sample_frames(
    video_path: &Path,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, MotionCheckError> {
    let duration = probe_duration_seconds(video_path)?;
    if duration <= 0.0 {
        return Err(MotionCheckError::ProbeFailed(format!(
            "video reported zero or unknown duration: {}",
            video_path.display()
        )));
    }

    // The "last" sample uses `-sseof -0.1` (seek 0.1s before EOF) instead of
    // an absolute timestamp. libvpx-vp9 typically reports a container
    // duration that overshoots the last packet pts by a few frames, so an
    // absolute `-ss duration - 0.05` lands past the decodable range and
    // produces zero frames. -sseof is decoded relative to EOF and always
    // resolves to a real frame.
    let samples: Vec<(SeekKind, f64)> = if duration < 0.4 {
        vec![(SeekKind::Ss, 0.0_f64), (SeekKind::SsEof, -0.1_f64)]
    } else {
        vec![
            (SeekKind::Ss, 0.0_f64),
            (SeekKind::Ss, duration / 2.0),
            (SeekKind::SsEof, -0.1_f64),
        ]
    };

    let mut frames = Vec::with_capacity(samples.len());
    let mut last_skip_reason: Option<String> = None;
    for (i, (kind, ts)) in samples.iter().enumerate() {
        let frame_path = out_dir.join(format!("frame-{:05}.png", i));
        let seek_flag = match kind {
            SeekKind::Ss => "-ss",
            SeekKind::SsEof => "-sseof",
        };
        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                seek_flag,
                &format!("{:.3}", ts),
                "-i",
                &video_path.to_string_lossy(),
                "-frames:v",
                "1",
                &frame_path.to_string_lossy(),
            ])
            .output()
            .map_err(|e| MotionCheckError::FfmpegSpawn(e.to_string()))?;
        if !output.status.success() {
            // Sample point failed (codec mismatch, seek-past-EOF, broken
            // input). Log the diagnostic so a downstream "0 frames extracted"
            // verdict can be traced back to the real cause; tolerate the
            // skip because adjacent samples may still succeed.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let summary = format!(
                "ffmpeg exit {} for sample {} ({:?} {:.3}): {}",
                output.status.code().unwrap_or(-1),
                i,
                kind,
                ts,
                truncate(&stderr, 200),
            );
            tracing::warn!(target: "motion_check", "{}", summary);
            last_skip_reason = Some(summary);
            continue;
        }
        if !frame_path.exists() {
            // ffmpeg exited 0 but produced no output -- typical when seeking
            // past the last decodable frame on libvpx-vp9 clips whose
            // container duration overshoots stream pts. Note it; downstream
            // can decide whether the remaining frames are enough.
            let summary = format!(
                "ffmpeg exited 0 but produced no frame for sample {} ({:?} {:.3})",
                i, kind, ts,
            );
            tracing::warn!(target: "motion_check", "{}", summary);
            last_skip_reason = Some(summary);
            continue;
        }
        frames.push(frame_path);
    }

    // Empty result means every sample point failed -- typically a missing
    // codec or unreadable input. Surface the last diagnostic instead of
    // returning an empty Vec, because the caller's "needs at least 2 frames"
    // path would otherwise mask an ffmpeg failure as a motion-check
    // rejection ("recording has 0 sample frames" reads identical to a
    // genuinely-degenerate clip from the user's perspective).
    if frames.is_empty() {
        return Err(MotionCheckError::FfmpegFailed(format!(
            "ffmpeg produced no sample frames for {}; check codec support. \
             Last skip reason: {}",
            video_path.display(),
            last_skip_reason.unwrap_or_else(|| "no samples attempted".to_string()),
        )));
    }
    Ok(frames)
}

#[derive(Debug, Clone, Copy)]
enum SeekKind {
    Ss,
    SsEof,
}

fn probe_duration_seconds(video_path: &Path) -> Result<f64, MotionCheckError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
            &video_path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| MotionCheckError::FfmpegSpawn(format!("ffprobe: {e}")))?;
    if !output.status.success() {
        return Err(MotionCheckError::ProbeFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let s = String::from_utf8_lossy(&output.stdout);
    let s = s.trim();
    if s.is_empty() || s == "N/A" {
        return Err(MotionCheckError::ProbeFailed(format!(
            "ffprobe returned no duration for {}",
            video_path.display()
        )));
    }
    s.parse::<f64>()
        .map_err(|e| MotionCheckError::ProbeFailed(format!("parse duration '{s}': {e}")))
}

fn pick_pair_indices(n: usize) -> Vec<(usize, usize)> {
    if n < 2 {
        return vec![];
    }
    if n == 2 {
        return vec![(0, 1)];
    }
    let middle = n / 2;
    let last = n - 1;
    vec![(0, middle), (middle, last), (0, last)]
}

#[derive(Debug, Clone)]
struct DiffStats {
    changed_fraction: f64,
    mean_diff: f64,
}

/// Compute mean luma difference and fraction of changed pixels between two
/// image files. Two ffmpeg invocations: one for mean diff (signalstats YAVG
/// of the difference image), one for changed fraction (geq-thresholded plane
/// then mean luma / 255 = fraction).
fn diff_stats(a: &Path, b: &Path) -> Result<DiffStats, MotionCheckError> {
    let mean_diff = signalstats_yavg(a, b, /* thresholded */ false)?;
    let yavg_thresh = signalstats_yavg(a, b, /* thresholded */ true)?;
    // YAVG over a {0,255}-thresholded plane = 255 * fraction_changed.
    let changed_fraction = (yavg_thresh / 255.0).clamp(0.0, 1.0);
    Ok(DiffStats {
        changed_fraction,
        mean_diff,
    })
}

fn signalstats_yavg(a: &Path, b: &Path, thresholded: bool) -> Result<f64, MotionCheckError> {
    let filter = if thresholded {
        format!(
            "[0:v][1:v]blend=all_mode=difference,format=gray,geq=lum='gt(lum(X\\,Y)\\,{})*255',signalstats,metadata=mode=print:file=-",
            LUMA_DIFF_THRESHOLD
        )
    } else {
        "[0:v][1:v]blend=all_mode=difference,format=gray,signalstats,metadata=mode=print:file=-"
            .to_string()
    };

    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-nostats",
            "-i",
            &a.to_string_lossy(),
            "-i",
            &b.to_string_lossy(),
            "-lavfi",
            &filter,
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|e| MotionCheckError::FfmpegSpawn(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(v) = parse_yavg(&stdout) {
        return Ok(v);
    }
    if let Some(v) = parse_yavg(&stderr) {
        return Ok(v);
    }
    // No YAVG line in either stream. Two reasons this can happen:
    //  1. ffmpeg failed mid-pipeline (exit nonzero, stderr has the diagnostic).
    //  2. ffmpeg exited 0 but the filter graph emitted no metadata (silent
    //     mismatch between our filter spec and the installed ffmpeg, or
    //     stdout/stderr routing changed in a future version).
    // Both cases must surface as an error rather than as a 0.0 reading.
    // Returning 0.0 here would silently flip every recording to "degenerate"
    // (changed_fraction = 0/255 = 0), masking ffmpeg breakage as a user-facing
    // motion-check rejection. The plan-parity audits caught this regression
    // mode in adversarial review.
    if !output.status.success() {
        return Err(MotionCheckError::FfmpegFailed(format!(
            "ffmpeg signalstats exited {}: {}",
            output.status.code().unwrap_or(-1),
            stderr,
        )));
    }
    Err(MotionCheckError::FfmpegFailed(format!(
        "ffmpeg signalstats produced no `lavfi.signalstats.YAVG=` line. \
         stdout (truncated): {}; stderr (truncated): {}",
        truncate(&stdout, 400),
        truncate(&stderr, 400),
    )))
}

fn truncate(s: &str, max: usize) -> String {
    // `max` is treated as a char count, not a byte count. Slicing `&s[..max]`
    // by byte index panics with "byte index is not a char boundary" when
    // `max` lands inside a multi-byte UTF-8 sequence (e.g. ffmpeg error
    // messages that include non-ASCII paths). Walk char boundaries instead.
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

fn parse_yavg(text: &str) -> Option<f64> {
    let needle = "lavfi.signalstats.YAVG=";
    let idx = text.find(needle)?;
    let tail = &text[idx + needle.len()..];
    let end = tail
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
        })
        .unwrap_or(tail.len());
    tail[..end].parse::<f64>().ok()
}

struct TempDir(PathBuf);

impl TempDir {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        global_infra::best_effort!(
            std::fs::remove_dir_all(&self.0),
            "motion_check temp dir cleanup"
        );
    }
}

fn tempdir() -> Result<TempDir, MotionCheckError> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = base.join(format!("mando-motion-{}-{}", pid, nonce));
    std::fs::create_dir_all(&dir).map_err(|e| MotionCheckError::TempDir(e.to_string()))?;
    Ok(TempDir(dir))
}

#[derive(Debug, thiserror::Error)]
pub enum MotionCheckError {
    #[error("ffprobe/ffmpeg duration probe failed: {0}")]
    ProbeFailed(String),
    #[error("ffmpeg failed: {0}")]
    FfmpegFailed(String),
    #[error("failed to spawn ffmpeg: {0}")]
    FfmpegSpawn(String),
    #[error("temp dir: {0}")]
    TempDir(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as Cmd;

    fn ffmpeg_available() -> bool {
        Cmd::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn make_static_webm(path: &Path) {
        // 3-second 320x240 static-color webm.
        let status = Cmd::new("ffmpeg")
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

    fn make_moving_webm(path: &Path) {
        // 3-second 320x240 webm where the entire frame cycles luma every frame.
        let status = Cmd::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=10:duration=3",
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
            .expect("ffmpeg moving webm build");
        assert!(status.success(), "moving webm build failed");
    }

    #[test]
    fn static_webm_is_degenerate() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg not available");
            return;
        }
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("static.webm");
        make_static_webm(&p);
        let v = check_video(&p).expect("check_video");
        assert_eq!(
            v.verdict,
            Verdict::Degenerate,
            "verdict={:?} reason={}",
            v.verdict,
            v.reason
        );
        assert!(v.changed_fraction < MIN_CHANGED_FRACTION);
    }

    #[test]
    fn moving_webm_is_ok() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg not available");
            return;
        }
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("moving.webm");
        make_moving_webm(&p);
        let v = check_video(&p).expect("check_video");
        assert_eq!(
            v.verdict,
            Verdict::Ok,
            "verdict={:?} reason={}",
            v.verdict,
            v.reason
        );
        assert!(v.changed_fraction > MIN_CHANGED_FRACTION);
    }

    fn make_single_region_webm(path: &Path) {
        // Reproduce the pr-977 borderline pattern in a Rust fixture: a
        // mostly-static frame where a tiny region flips on the last frame.
        // 320x240 = 76,800 pixels; flip 7x8 = 56 pixels = 0.073% — well
        // under MIN_CHANGED_FRACTION (0.1%), matches pr-977's empirical
        // 0.0008% scaled to a smaller canvas.
        let status = Cmd::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=0x808080:s=320x240:d=3",
                "-vf",
                "drawbox=x=10:y=20:w=7:h=8:color=0x808080:t=fill,\
                 drawbox=x=10:y=20:w=7:h=8:color=0xa0a0a0:t=fill:enable='gte(t,2.5)'",
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
            .expect("ffmpeg single-region webm build");
        assert!(status.success(), "single-region webm build failed");
    }

    #[test]
    fn single_region_change_is_degenerate() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg not available");
            return;
        }
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("region.webm");
        make_single_region_webm(&p);
        let v = check_video(&p).expect("check_video");
        assert_eq!(
            v.verdict,
            Verdict::Degenerate,
            "tiny-region change must reject (mirrors pr-977 0.0008% case): verdict={:?} reason={} changed={}",
            v.verdict,
            v.reason,
            v.changed_fraction,
        );
        assert!(v.changed_fraction < MIN_CHANGED_FRACTION);
    }

    #[test]
    fn truncate_walks_char_boundaries() {
        // ASCII: standard prefix-by-char-count.
        assert_eq!(super::truncate("hello world", 5), "hello…");
        // No truncation needed.
        assert_eq!(super::truncate("hi", 5), "hi");
        // Non-ASCII: must not panic on multi-byte UTF-8 (regression test for
        // greptile #3142716684; previous impl sliced by byte index and would
        // panic on `/Users/张三/screen.webm` in an ffmpeg stderr message).
        let s = "Path: /Users/张三/screen.webm — exit 1";
        let _ = super::truncate(s, 10); // does not panic
        let _ = super::truncate(s, 15); // does not panic
                                        // Truncate exactly on a multi-byte char.
        assert_eq!(super::truncate("ab张三", 3), "ab张…");
    }

    #[test]
    fn parse_yavg_extracts_value() {
        let sample =
            "frame:0 pts:0 pts_time:0\nlavfi.signalstats.YHIGH=0.1\nlavfi.signalstats.YAVG=42.5\n";
        let v = parse_yavg(sample);
        assert_eq!(v, Some(42.5));
    }

    #[test]
    fn pair_indices_basic() {
        assert_eq!(pick_pair_indices(0), vec![]);
        assert_eq!(pick_pair_indices(1), vec![]);
        assert_eq!(pick_pair_indices(2), vec![(0, 1)]);
        assert_eq!(pick_pair_indices(3), vec![(0, 1), (1, 2), (0, 2)]);
        assert_eq!(pick_pair_indices(10), vec![(0, 5), (5, 9), (0, 9)]);
    }
}
