//! Per-process Codex home setup for Codex pooled launchers.
//!
//! ChatGPT OAuth access tokens are JWT-shaped; Codex misroutes them through
//! the Agent Identity path when passed via `CODEX_ACCESS_TOKEN`. Instead we
//! materialize a temp `CODEX_HOME` with the picked `auth.json` and symlink
//! session state back to `~/.codex` so threads stay shared.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sqlx::Connection;

const DEFAULT_CODEX_HOME: &str = ".codex";
const MANAGED_HOME_PREFIX: &str = "mando-codex-home-";
/// Marker file written into every managed `CODEX_HOME` at materialization
/// time, recording the real source home it was materialized from (one line,
/// the resolved path). Cleanup/prune run later, sometimes with `CODEX_HOME`
/// still pointed at the now-doomed managed dir itself, which makes
/// `shared_codex_home()` fall back to the default `~/.codex` regardless of
/// the real source home used at materialization (e.g. a custom, non-default
/// `CODEX_HOME`). Reading this marker back lets rescue always target the
/// SAME home the session came from.
const SOURCE_HOME_MARKER: &str = ".mando-source-home";

/// True when `path` is a Mando-managed per-pick temp dir under the system temp folder.
pub(crate) fn is_managed_codex_home(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(MANAGED_HOME_PREFIX))
        && path.starts_with(std::env::temp_dir())
}

/// Remove a managed temp `CODEX_HOME` after tokens are synced. No-op for other paths.
///
/// Before deleting, rescue any top-level file/dir Codex created mid-session
/// that was never symlinked back to the shared home (symlinks are set up
/// once at materialization time in `symlink_shared_state`, so anything
/// Codex creates fresh under this `CODEX_HOME` — a new session log, a new
/// cache file — lives here as a real entry, not a symlink, and would
/// otherwise be lost to `remove_dir_all`).
pub(crate) async fn cleanup_managed_codex_home(path: &Path) -> io::Result<()> {
    if is_managed_codex_home(path) {
        if let Some(rescue_home) = rescue_target_for(path) {
            preserve_new_entries(path, &rescue_home);
            reconcile_rollout_paths(path, &rescue_home).await?;
        }
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Rewrite Codex's persisted rollout paths before deleting a managed home.
///
/// Codex records the lexical `CODEX_HOME` path in `state_5.sqlite`, even when
/// `sessions` is a symlink into the shared home. Once Mando removes the
/// managed directory, those otherwise-valid threads become impossible to
/// open or archive. Only rows whose replacement rollout already exists are
/// updated, and all changes commit together so a failed reconciliation keeps
/// the managed home available for recovery.
async fn reconcile_rollout_paths(managed_home: &Path, shared_home: &Path) -> io::Result<()> {
    let state_db = shared_home.join("state_5.sqlite");
    if !state_db.is_file() {
        return Ok(());
    }

    let options = SqliteConnectOptions::new()
        .filename(&state_db)
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(5));
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .map_err(sqlite_io_error)?;

    let threads_table: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'threads' LIMIT 1",
    )
    .fetch_optional(&mut connection)
    .await
    .map_err(sqlite_io_error)?;
    if threads_table.is_none() {
        return Ok(());
    }

    let managed_prefix = managed_home.to_string_lossy().into_owned();
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, rollout_path FROM threads WHERE instr(rollout_path, ?) = 1")
            .bind(&managed_prefix)
            .fetch_all(&mut connection)
            .await
            .map_err(sqlite_io_error)?;

    let mut replacements = Vec::new();
    for (thread_id, stale_path) in rows {
        let stale_path_buf = PathBuf::from(&stale_path);
        let Ok(relative_path) = stale_path_buf.strip_prefix(managed_home) else {
            continue;
        };
        let replacement = shared_home.join(relative_path);
        if !replacement.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "cannot reconcile Codex thread {thread_id}: replacement rollout is missing at {}",
                    replacement.display()
                ),
            ));
        }
        replacements.push((thread_id, stale_path, replacement));
    }

    if replacements.is_empty() {
        return Ok(());
    }

    let mut transaction = connection.begin().await.map_err(sqlite_io_error)?;
    for (thread_id, stale_path, replacement) in replacements {
        let result =
            sqlx::query("UPDATE threads SET rollout_path = ? WHERE id = ? AND rollout_path = ?")
                .bind(replacement.to_string_lossy().as_ref())
                .bind(&thread_id)
                .bind(&stale_path)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_io_error)?;
        if result.rows_affected() != 1 {
            return Err(io::Error::other(format!(
                "cannot reconcile Codex thread {thread_id}: rollout path changed concurrently"
            )));
        }
    }
    transaction.commit().await.map_err(sqlite_io_error)
}

fn sqlite_io_error(error: sqlx::Error) -> io::Error {
    io::Error::other(format!("Codex state database error: {error}"))
}

/// Determine where to rescue new files created inside a managed
/// `CODEX_HOME` before it is deleted: the `.mando-source-home` marker
/// written at materialization time, or `shared_codex_home()` when the
/// marker is missing or unreadable.
fn rescue_target_for(managed_home: &Path) -> Option<PathBuf> {
    match fs::read_to_string(managed_home.join(SOURCE_HOME_MARKER)) {
        Ok(contents) => {
            let trimmed = contents.trim();
            if trimmed.is_empty() {
                shared_codex_home().ok()
            } else {
                Some(PathBuf::from(trimmed))
            }
        }
        Err(_) => shared_codex_home().ok(),
    }
}

/// Move every top-level entry of `managed_home` that is (a) not a symlink
/// and (b) not mando-written into `shared_home`, skipping any name that
/// already exists there (the shared home wins; we never clobber it). Best
/// effort: a single entry failing to move does not stop the rest, and the
/// caller proceeds to remove `managed_home` regardless.
///
/// Skipped as mando-written rather than codex-created: `auth.json` (holds
/// pick-scoped tokens), its `auth.json.tmp.*` atomic-write siblings,
/// `config.toml` — `write_file_auth_config` writes a mando-generated
/// `config.toml` into the managed home whenever the shared home has none,
/// and rescuing it would silently install that override into the user's
/// personal `~/.codex` — and the `.mando-source-home` marker itself.
fn preserve_new_entries(managed_home: &Path, shared_home: &Path) {
    if fs::create_dir_all(shared_home).is_err() {
        return;
    }
    let Ok(entries) = fs::read_dir(managed_home) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "config.toml"
            || name_str.starts_with("auth.json")
            || name_str == SOURCE_HOME_MARKER
        {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let target = shared_home.join(&name);
        if fs::symlink_metadata(&target).is_ok() {
            // Shared home already has an entry with this name; leave the
            // managed-home copy behind for removal rather than clobber it.
            continue;
        }
        global_infra::best_effort!(
            relocate_entry(&entry.path(), &target),
            "preserve codex-created file before managed CODEX_HOME cleanup"
        );
    }
}

/// Move `src` to `dst`, falling back to copy+remove when they live on
/// different filesystems (`fs::rename` returns an error crossing devices,
/// e.g. the system temp folder vs. the user's home volume).
fn relocate_entry(src: &Path, dst: &Path) -> io::Result<()> {
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    if src.is_dir() {
        copy_dir_recursive(src, dst)?;
        fs::remove_dir_all(src)
    } else {
        fs::copy(src, dst)?;
        fs::remove_file(src)
    }
}

/// Build a per-shell Codex home: write `auth.json`, symlink everything else
/// from the user's real `~/.codex` so session history stays shared.
pub(crate) async fn materialize_codex_home(auth_json: &str) -> io::Result<PathBuf> {
    prune_stale_managed_homes().await;
    let real_home = shared_codex_home()?;
    ensure_shared_codex_home(&real_home)?;
    let tmp_home = std::env::temp_dir().join(format!(
        "mando-codex-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&tmp_home)?;
    fs::write(
        tmp_home.join(SOURCE_HOME_MARKER),
        real_home.to_string_lossy().as_ref(),
    )?;
    let auth_path = tmp_home.join("auth.json");
    fs::write(&auth_path, auth_json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&auth_path, fs::Permissions::from_mode(0o600))?;
    }
    symlink_shared_state(&real_home, &tmp_home)?;
    write_file_auth_config(&tmp_home)?;
    Ok(tmp_home)
}

/// Path to `auth.json` inside the active per-process `CODEX_HOME`.
pub(crate) fn codex_home_auth_json_path() -> io::Result<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "CODEX_HOME is not set (run pick --codex first)",
            )
        })?;
    Ok(home.join("auth.json"))
}

fn write_file_auth_config(tmp_home: &Path) -> io::Result<()> {
    let config_path = tmp_home.join("config.toml");
    if config_path.exists() {
        return Ok(());
    }
    fs::write(&config_path, "cli_auth_credentials_store = \"file\"\n")?;
    Ok(())
}

/// User-owned Codex home for shared session state (`~/.codex` by default).
/// Ignores a stale managed `CODEX_HOME` left over from a prior pick in the
/// same shell after `sync-codex` removed the temp directory.
fn shared_codex_home() -> io::Result<PathBuf> {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if !is_managed_codex_home(&path) {
                return Ok(path);
            }
        }
    }
    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(PathBuf::from(home).join(DEFAULT_CODEX_HOME))
}

fn ensure_shared_codex_home(real_home: &Path) -> io::Result<()> {
    fs::create_dir_all(real_home.join("sessions"))?;
    Ok(())
}

async fn prune_stale_managed_homes() {
    let temp = std::env::temp_dir();
    let Ok(entries) = fs::read_dir(&temp) else {
        return;
    };
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(86_400))
        .unwrap_or(std::time::UNIX_EPOCH);
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_managed_codex_home(&path) {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if modified > cutoff {
                    continue;
                }
            }
        }
        // Each stale managed dir carries its own source-home marker. Reuse
        // normal cleanup so rollout reconciliation remains a precondition
        // for deletion here too.
        global_infra::best_effort!(
            cleanup_managed_codex_home(&path).await,
            "prune stale managed CODEX_HOME"
        );
    }
}

fn symlink_shared_state(real_home: &Path, tmp_home: &Path) -> io::Result<()> {
    for entry in fs::read_dir(real_home)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "auth.json" {
            continue;
        }
        let target = entry.path();
        let link = tmp_home.join(&name);
        if link.exists() {
            continue;
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &link)?;
        }
        #[cfg(not(unix))]
        {
            if target.is_dir() {
                copy_dir_recursive(&target, &link)?;
            } else {
                fs::copy(&target, &link)?;
            }
        }
    }
    Ok(())
}

/// Recursively copy `src` into `dst`. Used on non-unix targets where
/// `symlink_shared_state` cannot create real symlinks, and as the
/// cross-filesystem fallback in `relocate_entry`.
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn shared_codex_home_ignores_stale_managed_codex_home() {
        let managed =
            std::env::temp_dir().join(format!("mando-codex-home-{}-stale", std::process::id()));
        fs::create_dir_all(&managed).expect("managed dir");
        let prev = std::env::var("CODEX_HOME").ok();
        std::env::set_var("CODEX_HOME", managed.to_string_lossy().as_ref());

        let home = shared_codex_home().expect("shared home");
        let expected = std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(DEFAULT_CODEX_HOME))
            .expect("HOME");
        assert_eq!(home, expected);

        if let Some(v) = prev {
            std::env::set_var("CODEX_HOME", v);
        } else {
            std::env::remove_var("CODEX_HOME");
        }
        let _ = fs::remove_dir_all(&managed);
    }

    #[tokio::test]
    async fn cleanup_preserves_new_top_level_file_into_shared_home() {
        let base = std::env::temp_dir().join(format!(
            "mando-codex-cleanup-preserve-{}",
            std::process::id()
        ));
        let shared = base.join(DEFAULT_CODEX_HOME);
        fs::create_dir_all(&shared).expect("shared dir");
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", base.to_string_lossy().as_ref());
        let prev_codex = std::env::var("CODEX_HOME").ok();
        std::env::remove_var("CODEX_HOME");

        let managed = std::env::temp_dir().join(format!(
            "mando-codex-home-{}-cleanup-preserve",
            std::process::id()
        ));
        fs::create_dir_all(&managed).expect("managed dir");
        fs::write(managed.join("auth.json"), "{}").expect("auth.json");
        fs::write(managed.join("auth.json.tmp.1.2"), "{}").expect("auth tmp sibling");
        fs::write(managed.join("config.toml"), "mando-written override").expect("config.toml");
        fs::write(managed.join("new-history.jsonl"), "session-data").expect("new file");

        cleanup_managed_codex_home(&managed).await.expect("cleanup");

        assert!(!managed.exists(), "managed home should be removed");
        let preserved = shared.join("new-history.jsonl");
        assert!(
            preserved.is_file(),
            "new file should be rescued into shared home"
        );
        assert_eq!(
            fs::read_to_string(&preserved).expect("read preserved file"),
            "session-data"
        );
        // Mando-written files must not be copied back: auth.json holds
        // pick-scoped tokens the daemon already received via sync, its
        // .tmp.* siblings are atomic-write leftovers, and config.toml may
        // be the mando-generated file-auth-store override.
        assert!(!shared.join("auth.json").exists());
        assert!(!shared.join("auth.json.tmp.1.2").exists());
        assert!(!shared.join("config.toml").exists());

        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        }
        if let Some(v) = prev_codex {
            std::env::set_var("CODEX_HOME", v);
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn cleanup_does_not_clobber_existing_shared_home_entry() {
        let base =
            std::env::temp_dir().join(format!("mando-codex-cleanup-skip-{}", std::process::id()));
        let shared = base.join(DEFAULT_CODEX_HOME);
        fs::create_dir_all(&shared).expect("shared dir");
        fs::write(shared.join("config.toml"), "original").expect("existing shared file");
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", base.to_string_lossy().as_ref());
        let prev_codex = std::env::var("CODEX_HOME").ok();
        std::env::remove_var("CODEX_HOME");

        let managed = std::env::temp_dir().join(format!(
            "mando-codex-home-{}-cleanup-skip",
            std::process::id()
        ));
        fs::create_dir_all(&managed).expect("managed dir");
        fs::write(managed.join("config.toml"), "changed-in-session").expect("managed file");

        cleanup_managed_codex_home(&managed).await.expect("cleanup");

        assert!(!managed.exists(), "managed home should be removed");
        assert_eq!(
            fs::read_to_string(shared.join("config.toml")).expect("read shared file"),
            "original",
            "shared home entry must win over the managed-home copy"
        );

        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        }
        if let Some(v) = prev_codex {
            std::env::set_var("CODEX_HOME", v);
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn materialize_creates_sessions_when_shared_home_missing() {
        let base =
            std::env::temp_dir().join(format!("mando-codex-pick-missing-{}", std::process::id()));
        let shared = base.join(DEFAULT_CODEX_HOME);
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", base.to_string_lossy().as_ref());
        let prev_codex = std::env::var("CODEX_HOME").ok();
        std::env::remove_var("CODEX_HOME");

        let tmp = materialize_codex_home(r#"{"auth_mode":"chatgpt","tokens":{}}"#)
            .await
            .expect("materialize");
        assert!(shared.join("sessions").is_dir());
        #[cfg(unix)]
        {
            assert!(tmp.join("sessions").is_symlink());
        }

        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        }
        if let Some(v) = prev_codex {
            std::env::set_var("CODEX_HOME", v);
        }
        // `tmp` (the managed CODEX_HOME) lives directly under the real system
        // temp dir, not under `base` — clean it up separately or it leaks a
        // real `mando-codex-home-*` dir outside the test's own sandbox.
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn materialize_writes_auth_and_symlinks_sessions() {
        let base =
            std::env::temp_dir().join(format!("mando-codex-pick-test-{}", std::process::id()));
        fs::create_dir_all(&base).expect("base dir");
        let real = base.join("real-codex");
        let sessions = real.join("sessions");
        fs::create_dir_all(&sessions).expect("sessions dir");
        fs::write(real.join("auth.json"), r#"{"auth_mode":"chatgpt"}"#).expect("auth");
        fs::write(real.join("config.toml"), "model = \"gpt-5\"\n").expect("config");
        fs::write(sessions.join("thread.json"), "{}").expect("thread");

        let prev = std::env::var("CODEX_HOME").ok();
        std::env::set_var("CODEX_HOME", real.to_string_lossy().as_ref());

        let tmp = materialize_codex_home(r#"{"auth_mode":"chatgpt","tokens":{}}"#)
            .await
            .expect("materialize");
        assert!(tmp.join("auth.json").is_file());
        assert_eq!(
            fs::read_to_string(tmp.join(SOURCE_HOME_MARKER)).expect("marker"),
            real.to_string_lossy(),
            "marker must record the real source home used at materialization"
        );
        #[cfg(unix)]
        {
            let link = tmp.join("sessions");
            assert!(link.is_symlink());
            assert!(link.join("thread.json").is_file());
            assert!(tmp.join("config.toml").is_symlink());
            let config = fs::read_to_string(tmp.join("config.toml")).expect("config");
            assert!(config.contains("model = \"gpt-5\""));
        }

        if let Some(v) = prev {
            std::env::set_var("CODEX_HOME", v);
        } else {
            std::env::remove_var("CODEX_HOME");
        }

        // `tmp` (the managed CODEX_HOME) lives directly under the real
        // system temp dir, not under `base` — clean it up separately or it
        // leaks a real `mando-codex-home-*` dir outside the test's sandbox.
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn cleanup_rescues_into_marker_path_not_shared_codex_home() {
        let base =
            std::env::temp_dir().join(format!("mando-codex-marker-rescue-{}", std::process::id()));
        // What `shared_codex_home()` would resolve to (HOME/.codex) if the
        // marker were ignored — a decoy the test proves rescue does NOT use.
        let decoy_shared = base.join(DEFAULT_CODEX_HOME);
        let real_source_home = base.join("real-source-home");
        fs::create_dir_all(&decoy_shared).expect("decoy shared dir");
        fs::create_dir_all(&real_source_home).expect("real source home dir");

        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", base.to_string_lossy().as_ref());
        let prev_codex = std::env::var("CODEX_HOME").ok();
        std::env::remove_var("CODEX_HOME");

        let managed = std::env::temp_dir().join(format!(
            "mando-codex-home-{}-marker-rescue",
            std::process::id()
        ));
        fs::create_dir_all(&managed).expect("managed dir");
        fs::write(
            managed.join(SOURCE_HOME_MARKER),
            real_source_home.to_string_lossy().as_ref(),
        )
        .expect("write marker");
        fs::write(managed.join("new-history.jsonl"), "session-data").expect("new file");

        cleanup_managed_codex_home(&managed).await.expect("cleanup");

        assert!(!managed.exists(), "managed home should be removed");
        assert!(
            real_source_home.join("new-history.jsonl").is_file(),
            "new file must be rescued into the marker's source home"
        );
        assert!(
            !decoy_shared.join("new-history.jsonl").exists(),
            "must not fall back to shared_codex_home() when a marker is present"
        );
        assert!(
            !real_source_home.join(SOURCE_HOME_MARKER).exists(),
            "the marker file itself must not be rescued"
        );

        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        }
        if let Some(v) = prev_codex {
            std::env::set_var("CODEX_HOME", v);
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn cleanup_falls_back_to_shared_home_when_marker_missing() {
        let base =
            std::env::temp_dir().join(format!("mando-codex-marker-missing-{}", std::process::id()));
        let shared = base.join(DEFAULT_CODEX_HOME);
        fs::create_dir_all(&shared).expect("shared dir");
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", base.to_string_lossy().as_ref());
        let prev_codex = std::env::var("CODEX_HOME").ok();
        std::env::remove_var("CODEX_HOME");

        let managed = std::env::temp_dir().join(format!(
            "mando-codex-home-{}-marker-missing",
            std::process::id()
        ));
        fs::create_dir_all(&managed).expect("managed dir");
        fs::write(managed.join("new-history.jsonl"), "session-data").expect("new file");

        cleanup_managed_codex_home(&managed).await.expect("cleanup");

        assert!(!managed.exists(), "managed home should be removed");
        assert!(
            shared.join("new-history.jsonl").is_file(),
            "no marker must fall back to rescuing into shared_codex_home()"
        );

        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        }
        if let Some(v) = prev_codex {
            std::env::set_var("CODEX_HOME", v);
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn cleanup_rewrites_managed_rollout_path_to_shared_home() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let shared = std::env::temp_dir().join(format!("mando-codex-shared-{suffix}"));
        let managed = std::env::temp_dir().join(format!("mando-codex-home-{suffix}"));
        let relative_rollout = PathBuf::from("sessions/2026/08/23/thread.jsonl");
        let shared_rollout = shared.join(&relative_rollout);
        fs::create_dir_all(shared_rollout.parent().expect("rollout parent"))
            .expect("shared sessions");
        fs::create_dir_all(&managed).expect("managed home");
        fs::write(&shared_rollout, "session-data").expect("rollout");
        fs::write(
            managed.join(SOURCE_HOME_MARKER),
            shared.to_string_lossy().as_ref(),
        )
        .expect("source marker");

        let state_db = shared.join("state_5.sqlite");
        let options = SqliteConnectOptions::new()
            .filename(&state_db)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options)
            .await
            .expect("state db");
        sqlx::query("CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT NOT NULL)")
            .execute(&mut connection)
            .await
            .expect("threads table");
        let stale_rollout = managed.join(&relative_rollout);
        sqlx::query("INSERT INTO threads (id, rollout_path) VALUES (?, ?)")
            .bind("thread-1")
            .bind(stale_rollout.to_string_lossy().as_ref())
            .execute(&mut connection)
            .await
            .expect("thread row");
        connection.close().await.expect("close state db");

        cleanup_managed_codex_home(&managed).await.expect("cleanup");

        assert!(!managed.exists(), "managed home should be removed");
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&state_db)
                .create_if_missing(false),
        )
        .await
        .expect("reopen state db");
        let (rollout_path,): (String,) =
            sqlx::query_as("SELECT rollout_path FROM threads WHERE id = ?")
                .bind("thread-1")
                .fetch_one(&mut connection)
                .await
                .expect("rewritten thread row");
        assert_eq!(PathBuf::from(rollout_path), shared_rollout);
        connection.close().await.expect("close state db");
        let _ = fs::remove_dir_all(&shared);
    }

    #[tokio::test]
    async fn cleanup_keeps_managed_home_when_replacement_rollout_is_missing() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let shared = std::env::temp_dir().join(format!("mando-codex-shared-missing-{suffix}"));
        let managed = std::env::temp_dir().join(format!("mando-codex-home-missing-{suffix}"));
        fs::create_dir_all(&shared).expect("shared home");
        fs::create_dir_all(&managed).expect("managed home");
        fs::write(
            managed.join(SOURCE_HOME_MARKER),
            shared.to_string_lossy().as_ref(),
        )
        .expect("source marker");

        let state_db = shared.join("state_5.sqlite");
        let options = SqliteConnectOptions::new()
            .filename(&state_db)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options)
            .await
            .expect("state db");
        sqlx::query("CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT NOT NULL)")
            .execute(&mut connection)
            .await
            .expect("threads table");
        sqlx::query("INSERT INTO threads (id, rollout_path) VALUES (?, ?)")
            .bind("thread-1")
            .bind(
                managed
                    .join("sessions/missing.jsonl")
                    .to_string_lossy()
                    .as_ref(),
            )
            .execute(&mut connection)
            .await
            .expect("thread row");
        connection.close().await.expect("close state db");

        let error = cleanup_managed_codex_home(&managed)
            .await
            .expect_err("missing rollout must block cleanup");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(managed.exists(), "managed home must remain recoverable");
        let _ = fs::remove_dir_all(&managed);
        let _ = fs::remove_dir_all(&shared);
    }
}
