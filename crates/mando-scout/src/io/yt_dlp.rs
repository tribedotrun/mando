//! Managed yt-dlp binary — download-on-first-use with version pinning.
//!
//! Downloads the standalone macOS universal binary from GitHub releases.
//! No system Python required. The standalone binary bundles its own Python
//! runtime via PyInstaller.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;
use tracing::{info, warn};

const VERSION: &str = "2026.03.17";
const EXPECTED_SHA256: &str = "e80c47b3ce712acee51d5e3d4eace2d181b44d38f1942c3a32e3c7ff53cd9ed5";

/// Process-wide guard: at most one download runs, all callers share the result.
static INIT: OnceCell<PathBuf> = OnceCell::const_new();

fn download_url() -> String {
    format!("https://github.com/yt-dlp/yt-dlp/releases/download/{VERSION}/yt-dlp_macos")
}

fn bin_path() -> PathBuf {
    mando_config::bin_dir().join("yt-dlp")
}

fn version_path() -> PathBuf {
    mando_config::bin_dir().join(".yt-dlp.version")
}

/// Ensure yt-dlp is available, downloading if missing or outdated.
/// Returns the path to the executable.
/// Concurrent callers are serialized — only one download runs.
pub async fn ensure_yt_dlp() -> Result<PathBuf> {
    // OnceCell ensures only one download runs; subsequent callers wait and share the result.
    INIT.get_or_try_init(|| async {
        let path = bin_path();

        // Fast path: binary exists and version matches.
        if path.exists() {
            match tokio::fs::read_to_string(version_path()).await {
                Ok(v) if v.trim() == VERSION => return Ok(path),
                Ok(v) => {
                    info!(
                        current = v.trim(),
                        target = VERSION,
                        "yt-dlp version mismatch, updating"
                    );
                }
                Err(e) => {
                    warn!(error = %e, "yt-dlp version file unreadable, re-downloading");
                }
            }
        }

        download_and_verify().await?;
        Ok(path)
    })
    .await
    .cloned()
}

async fn download_and_verify() -> Result<()> {
    let bin_dir = mando_config::bin_dir();
    tokio::fs::create_dir_all(&bin_dir).await?;

    let url = download_url();
    info!(%url, "downloading yt-dlp {VERSION}");

    let client = {
        static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        CLIENT.get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("failed to build HTTP client")
        })
    };

    let resp = client
        .get(&url)
        .send()
        .await
        .context("yt-dlp download request failed")?;

    if !resp.status().is_success() {
        bail!("yt-dlp download failed: HTTP {}", resp.status());
    }

    let bytes = resp
        .bytes()
        .await
        .context("yt-dlp download body read failed")?;

    // Verify SHA-256.
    let hash = hex_sha256(&bytes);
    if hash != EXPECTED_SHA256 {
        bail!("yt-dlp checksum mismatch: expected {EXPECTED_SHA256}, got {hash}");
    }

    // Atomic write: temp file with unique name → rename.
    let tmp_path = bin_dir.join(format!(".yt-dlp.downloading.{}", std::process::id()));
    tokio::fs::write(&tmp_path, &bytes).await?;

    // chmod +x
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) =
            tokio::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755)).await
        {
            tokio::fs::remove_file(&tmp_path).await.ok();
            return Err(e.into());
        }
    }

    // Write version file first — the rename is the commit point.
    // If we crash after version write but before rename, next startup
    // sees version mismatch + missing binary → re-downloads cleanly.
    if let Err(e) = tokio::fs::write(version_path(), VERSION).await {
        tokio::fs::remove_file(&tmp_path).await.ok();
        return Err(anyhow::anyhow!("failed to write yt-dlp version file: {e}"));
    }

    if let Err(e) = tokio::fs::rename(&tmp_path, bin_path()).await {
        tokio::fs::remove_file(&tmp_path).await.ok();
        tokio::fs::remove_file(version_path()).await.ok();
        return Err(anyhow::anyhow!("failed to install yt-dlp binary: {e}"));
    }

    info!(
        version = VERSION,
        "yt-dlp installed to {}",
        bin_path().display()
    );
    Ok(())
}

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}
