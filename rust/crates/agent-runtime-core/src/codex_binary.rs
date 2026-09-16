//! Codex CLI binary resolution shared by runtime and login flows.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

const CODEX_APP_BUNDLE_BINARY: &str = "/Applications/Codex.app/Contents/Resources/codex";

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
                module = "agent-runtime-core",
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
