//! Claude binary resolution.

use std::path::{Path, PathBuf};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_returns_non_empty() {
        let path = resolve_claude_binary();
        assert!(!path.as_os_str().is_empty());
    }
}
