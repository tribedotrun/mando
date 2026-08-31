//! Claude CLI binary resolution.

use std::path::{Path, PathBuf};

/// Resolve the `claude` CLI binary path.
///
/// Search order:
/// 1. `MANDO_CC_CLAUDE_BIN` when the path exists
/// 2. `which claude` (PATH lookup)
/// 3. common user and global install locations
/// 4. bare `claude`
pub fn resolve_claude_binary() -> PathBuf {
    if let Ok(value) = std::env::var("MANDO_CC_CLAUDE_BIN") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() && (path.is_absolute() || path.exists()) {
            return path;
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
    for candidate in candidates {
        if Path::new(&candidate).exists() {
            return PathBuf::from(candidate);
        }
    }

    PathBuf::from("claude")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_returns_non_empty() {
        assert!(!resolve_claude_binary().as_os_str().is_empty());
    }
}
