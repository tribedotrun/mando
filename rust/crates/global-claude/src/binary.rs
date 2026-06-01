//! Agent CLI binary resolution.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

const CODEX_APP_BUNDLE_BINARY: &str = "/Applications/Codex.app/Contents/Resources/codex";

/// Resolve the `claude` CLI binary path.
///
/// If `MANDO_CC_CLAUDE_BIN` is set to an existing executable path, it is used
/// (integration tests use `mando-cc-mock`).
///
/// Search order:
/// 1. `MANDO_CC_CLAUDE_BIN` when the path exists
/// 2. `which claude` (PATH lookup)
/// 3. `~/.npm-global/bin/claude`
/// 4. `~/.local/bin/claude`
/// 5. `/usr/local/bin/claude`
/// 6. Bare `"claude"` fallback
pub fn resolve_claude_binary() -> PathBuf {
    if let Ok(p) = std::env::var("MANDO_CC_CLAUDE_BIN") {
        let pb = PathBuf::from(&p);
        if pb.as_os_str().is_empty() {
            // ignore empty
        } else if pb.is_absolute() || pb.exists() {
            return pb;
        }
    }

    if let Ok(output) = std::process::Command::new("which").arg("claude").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return PathBuf::from(path);
            }
        }
    }

    let candidates: Vec<String> = if let Ok(home) = std::env::var("HOME") {
        vec![
            format!("{home}/.npm-global/bin/claude"),
            format!("{home}/.local/bin/claude"),
            "/usr/local/bin/claude".to_string(),
        ]
    } else {
        vec!["/usr/local/bin/claude".to_string()]
    };
    for c in &candidates {
        if Path::new(c).exists() {
            return PathBuf::from(c);
        }
    }

    PathBuf::from("claude")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCodexBinary {
    path: PathBuf,
    path_env: Option<OsString>,
}

impl ResolvedCodexBinary {
    fn new(path: PathBuf) -> Self {
        let path_env = codex_path_env_for(&path);
        Self { path, path_env }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn path_env(&self) -> Option<&OsStr> {
        self.path_env.as_deref()
    }
}

/// Resolve the `codex` CLI binary path and any PATH override it needs.
///
/// Installed Mando runs under launchd, so it can have a minimal `PATH` that
/// does not include shell-managed Node locations such as nvm. Search explicit
/// and stable install locations before falling back to a bare PATH lookup name.
/// When the resolved executable is a Node wrapper, its parent directory is
/// prepended to the child PATH so `/usr/bin/env node` can find the matching
/// `node` binary.
///
/// Search order:
/// 1. `MANDO_CODEX_BIN` when the path is absolute or exists
/// 2. `which codex` (current PATH lookup)
/// 3. Common user/global bin directories
/// 4. `~/.nvm/versions/node/*/bin/codex`
/// 5. `/Applications/Codex.app/Contents/Resources/codex`
/// 6. Bare `"codex"` fallback
pub fn resolve_codex_binary() -> ResolvedCodexBinary {
    let override_value = std::env::var_os("MANDO_CODEX_BIN");
    let path_lookup = lookup_binary_on_path("codex");
    let home = std::env::var_os("HOME");
    let fallback_candidates = codex_fallback_candidates(home.as_deref());
    resolve_codex_binary_from(override_value, path_lookup, fallback_candidates)
}

fn resolve_codex_binary_from(
    override_value: Option<OsString>,
    path_lookup: Option<PathBuf>,
    fallback_candidates: Vec<PathBuf>,
) -> ResolvedCodexBinary {
    if let Some(path) = resolve_override_path(override_value) {
        return ResolvedCodexBinary::new(path);
    }
    if let Some(path) = path_lookup {
        return ResolvedCodexBinary::new(path);
    }
    for candidate in fallback_candidates {
        if is_existing_executable(&candidate) {
            return ResolvedCodexBinary::new(candidate);
        }
    }
    ResolvedCodexBinary::new(PathBuf::from("codex"))
}

fn resolve_override_path(value: Option<OsString>) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok();
    resolve_override_path_from(value, cwd.as_deref())
}

fn resolve_override_path_from(value: Option<OsString>, cwd: Option<&Path>) -> Option<PathBuf> {
    let value = value?;
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return Some(path);
    }
    let absolute = cwd?.join(&path);
    if !absolute.exists() {
        return None;
    }
    Some(absolute.canonicalize().unwrap_or(absolute))
}

fn lookup_binary_on_path(binary: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("which")
        .arg(binary)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn codex_fallback_candidates(home: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(home) = home {
        let home = PathBuf::from(home);
        candidates.push(home.join(".npm-global/bin/codex"));
        candidates.push(home.join(".local/bin/codex"));
        candidates.extend(nvm_codex_candidates(&home));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/codex"));
    candidates.push(PathBuf::from("/usr/local/bin/codex"));
    candidates.push(PathBuf::from(CODEX_APP_BUNDLE_BINARY));
    candidates
}

fn nvm_codex_candidates(home: &Path) -> Vec<PathBuf> {
    nvm_node_bin_dirs(home)
        .into_iter()
        .map(|bin_dir| bin_dir.join("codex"))
        .collect()
}

fn nvm_node_bin_dirs(home: &Path) -> Vec<PathBuf> {
    let versions_dir = home.join(".nvm/versions/node");
    let mut candidates: Vec<(Vec<u64>, PathBuf)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(versions_dir) else {
        return Vec::new();
    };
    for entry in entries.filter_map(Result::ok) {
        candidates.push((
            nvm_node_version_key(&entry.file_name()),
            entry.path().join("bin"),
        ));
    }
    candidates.sort_by(|(left_key, left_path), (right_key, right_path)| {
        right_key
            .cmp(left_key)
            .then_with(|| right_path.cmp(left_path))
    });
    candidates.into_iter().map(|(_, path)| path).collect()
}

fn nvm_node_version_key(name: &std::ffi::OsStr) -> Vec<u64> {
    name.to_string_lossy()
        .trim_start_matches('v')
        .split(|ch: char| !ch.is_ascii_digit())
        .filter_map(|part| {
            if part.is_empty() {
                None
            } else {
                part.parse::<u64>().ok()
            }
        })
        .collect()
}

fn codex_path_env_for(path: &Path) -> Option<OsString> {
    let bin_dir = path.parent()?;
    if bin_dir.as_os_str().is_empty() {
        return None;
    }

    let home = std::env::var_os("HOME");
    codex_path_env_for_from(
        bin_dir,
        std::env::var_os("PATH"),
        node_fallback_bin_dirs(home.as_deref()),
    )
}

fn codex_path_env_for_from(
    bin_dir: &Path,
    current_path: Option<OsString>,
    node_bin_dirs: Vec<PathBuf>,
) -> Option<OsString> {
    let mut entries = Vec::new();
    push_unique_path(&mut entries, bin_dir.to_path_buf());
    for node_bin_dir in node_bin_dirs {
        push_unique_path(&mut entries, node_bin_dir);
    }
    if let Some(current_path) = current_path {
        for path in std::env::split_paths(&current_path) {
            push_unique_path(&mut entries, path);
        }
    }
    match std::env::join_paths(entries) {
        Ok(path_env) => Some(path_env),
        Err(error) => {
            tracing::warn!(
                module = "global-claude-lib",
                path = %bin_dir.display(),
                %error,
                "failed to build Codex child PATH"
            );
            None
        }
    }
}

fn node_fallback_bin_dirs(home: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path_node_dir) = lookup_binary_on_path("node").and_then(|path| {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
    }) {
        dirs.push(path_node_dir);
    }
    if let Some(home) = home {
        dirs.extend(nvm_node_bin_dirs(&PathBuf::from(home)));
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.retain(|dir| is_existing_executable(&dir.join("node")));
    dirs
}

fn push_unique_path(entries: &mut Vec<PathBuf>, path: PathBuf) {
    if !entries.iter().any(|entry| entry == &path) {
        entries.push(path);
    }
}

pub fn apply_codex_binary_env(command: &mut tokio::process::Command, codex: &ResolvedCodexBinary) {
    if let Some(path_env) = codex.path_env() {
        command.env("PATH", path_env);
    }
}

#[cfg(unix)]
fn is_existing_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_existing_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_returns_non_empty() {
        let path = resolve_claude_binary();
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn resolve_codex_prefers_explicit_override() -> anyhow::Result<()> {
        let dir = unique_test_dir("codex-override");
        std::fs::create_dir_all(&dir)?;
        let codex = dir.join("codex");
        std::fs::write(&codex, "#!/bin/sh\n")?;

        let resolved =
            resolve_codex_binary_from(Some(codex.clone().into_os_string()), None, Vec::new());

        assert_eq!(resolved.path(), codex);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn resolve_codex_makes_relative_override_absolute() -> anyhow::Result<()> {
        let dir = unique_test_dir("codex-relative-override");
        let codex = dir.join("bin/codex");
        let parent = codex
            .parent()
            .ok_or_else(|| anyhow::anyhow!("test codex path missing parent"))?;
        std::fs::create_dir_all(parent)?;
        std::fs::write(&codex, "#!/bin/sh\n")?;
        let relative = OsString::from("bin/codex");

        let resolved = resolve_override_path_from(Some(relative), Some(&dir))
            .ok_or_else(|| anyhow::anyhow!("relative override did not resolve"))?;

        assert!(resolved.is_absolute());
        assert_eq!(resolved, codex.canonicalize()?);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn resolve_codex_uses_path_lookup_before_fallbacks() -> anyhow::Result<()> {
        let dir = unique_test_dir("codex-path");
        std::fs::create_dir_all(&dir)?;
        let codex = dir.join("codex");
        std::fs::write(&codex, "#!/bin/sh\n")?;

        let resolved = resolve_codex_binary_from(None, Some(codex.clone()), Vec::new());

        assert_eq!(resolved.path(), codex);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn resolve_codex_uses_executable_fallback_candidate() -> anyhow::Result<()> {
        let dir = unique_test_dir("codex-fallback");
        std::fs::create_dir_all(&dir)?;
        let codex = dir.join("codex");
        std::fs::write(&codex, "#!/bin/sh\n")?;
        make_executable(&codex)?;

        let resolved = resolve_codex_binary_from(None, None, vec![codex.clone()]);

        assert_eq!(resolved.path(), codex);
        let path_env = resolved
            .path_env()
            .ok_or_else(|| anyhow::anyhow!("resolved Codex binary missing child PATH"))?;
        assert_eq!(
            std::env::split_paths(path_env).next().as_deref(),
            codex.parent()
        );
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn codex_fallback_candidates_scan_nvm_versions() -> anyhow::Result<()> {
        let dir = unique_test_dir("codex-nvm");
        let codex = dir.join(".nvm/versions/node/v24.14.0/bin/codex");
        let parent = codex
            .parent()
            .ok_or_else(|| anyhow::anyhow!("test codex path missing parent"))?;
        std::fs::create_dir_all(parent)?;
        std::fs::write(&codex, "#!/bin/sh\n")?;

        let candidates = codex_fallback_candidates(Some(dir.as_os_str()));

        assert!(candidates.iter().any(|candidate| candidate == &codex));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn nvm_codex_candidates_sort_versions_numerically() -> anyhow::Result<()> {
        let dir = unique_test_dir("codex-nvm-sort");
        let old_codex = dir.join(".nvm/versions/node/v9.99.0/bin/codex");
        let new_codex = dir.join(".nvm/versions/node/v24.14.0/bin/codex");
        let old_parent = old_codex
            .parent()
            .ok_or_else(|| anyhow::anyhow!("old test codex path missing parent"))?;
        let new_parent = new_codex
            .parent()
            .ok_or_else(|| anyhow::anyhow!("new test codex path missing parent"))?;
        std::fs::create_dir_all(old_parent)?;
        std::fs::create_dir_all(new_parent)?;
        std::fs::write(&old_codex, "#!/bin/sh\n")?;
        std::fs::write(&new_codex, "#!/bin/sh\n")?;

        let candidates = nvm_codex_candidates(&dir);

        assert_eq!(candidates.first(), Some(&new_codex));
        assert_eq!(candidates.get(1), Some(&old_codex));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn resolve_codex_returns_bare_name_without_candidates() {
        let resolved = resolve_codex_binary_from(Some(OsString::new()), None, Vec::new());

        assert_eq!(resolved.path(), Path::new("codex"));
        assert_eq!(resolved.path_env(), None);
    }

    #[test]
    fn codex_path_env_includes_node_bin_dirs_before_current_path() -> anyhow::Result<()> {
        let dir = unique_test_dir("codex-path-env");
        let codex_bin = dir.join(".npm-global/bin");
        let node_bin = dir.join(".nvm/versions/node/v24.14.0/bin");
        std::fs::create_dir_all(&codex_bin)?;
        std::fs::create_dir_all(&node_bin)?;

        let path_env = codex_path_env_for_from(
            &codex_bin,
            Some(OsString::from("/usr/bin:/bin")),
            vec![node_bin.clone()],
        )
        .ok_or_else(|| anyhow::anyhow!("child PATH was not built"))?;
        let entries = std::env::split_paths(&path_env).collect::<Vec<_>>();

        assert_eq!(entries.first(), Some(&codex_bin));
        assert_eq!(entries.get(1), Some(&node_bin));
        assert_eq!(entries.get(2), Some(&PathBuf::from("/usr/bin")));
        assert_eq!(entries.get(3), Some(&PathBuf::from("/bin")));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>();
        std::env::temp_dir().join(format!("mando-{name}-{}-{}", std::process::id(), thread))
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions)?;
        Ok(())
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) -> anyhow::Result<()> {
        Ok(())
    }
}
