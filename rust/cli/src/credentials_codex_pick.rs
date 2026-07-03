//! Per-process Codex home setup for `mando credentials pick --codex`.
//!
//! ChatGPT OAuth access tokens are JWT-shaped; Codex misroutes them through
//! the Agent Identity path when passed via `CODEX_ACCESS_TOKEN`. Instead we
//! materialize a temp `CODEX_HOME` with the picked `auth.json` and symlink
//! session state back to `~/.codex` so threads stay shared.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const DEFAULT_CODEX_HOME: &str = ".codex";
const MANAGED_HOME_PREFIX: &str = "mando-codex-home-";

/// True when `path` is a Mando-managed per-pick temp dir under the system temp folder.
pub(crate) fn is_managed_codex_home(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(MANAGED_HOME_PREFIX))
        && path.starts_with(std::env::temp_dir())
}

/// Remove a managed temp `CODEX_HOME` after tokens are synced. No-op for other paths.
pub(crate) fn cleanup_managed_codex_home(path: &Path) -> io::Result<()> {
    if is_managed_codex_home(path) {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Build a per-shell Codex home: write `auth.json`, symlink everything else
/// from the user's real `~/.codex` so session history stays shared.
pub(crate) fn materialize_codex_home(auth_json: &str) -> io::Result<PathBuf> {
    prune_stale_managed_homes();
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

fn prune_stale_managed_homes() {
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
        global_infra::best_effort!(fs::remove_dir_all(&path), "prune stale managed CODEX_HOME");
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

#[cfg(not(unix))]
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

    #[test]
    fn materialize_creates_sessions_when_shared_home_missing() {
        let base =
            std::env::temp_dir().join(format!("mando-codex-pick-missing-{}", std::process::id()));
        let shared = base.join(DEFAULT_CODEX_HOME);
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", base.to_string_lossy().as_ref());
        let prev_codex = std::env::var("CODEX_HOME").ok();
        std::env::remove_var("CODEX_HOME");

        let tmp =
            materialize_codex_home(r#"{"auth_mode":"chatgpt","tokens":{}}"#).expect("materialize");
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
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn materialize_writes_auth_and_symlinks_sessions() {
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

        let tmp =
            materialize_codex_home(r#"{"auth_mode":"chatgpt","tokens":{}}"#).expect("materialize");
        assert!(tmp.join("auth.json").is_file());
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

        let _ = fs::remove_dir_all(&base);
    }
}
