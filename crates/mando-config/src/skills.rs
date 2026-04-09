//! Sync bundled prod skills to `~/.claude/skills/mando-*`.
//!
//! Skills are compiled into the binary via `include_str!` and written to disk
//! at daemon startup so Claude Code workers can invoke them as slash commands.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tracing::{info, warn};

struct BundledSkill {
    name: &'static str,
    files: &'static [(&'static str, &'static str, bool)], // (filename, content, executable)
}

// -- Bundled skill content (compiled into binary) --

const SKILLS: &[BundledSkill] = &[
    BundledSkill {
        name: "mando-pr",
        files: &[
            (
                "SKILL.md",
                include_str!("../../../bundled-skills/mando-pr/SKILL.md"),
                false,
            ),
            (
                "pr_status.py",
                include_str!("../../../bundled-skills/mando-pr/pr_status.py"),
                false,
            ),
            (
                "gh_async.py",
                include_str!("../../../bundled-skills/mando-pr/gh_async.py"),
                false,
            ),
        ],
    },
    BundledSkill {
        name: "mando-pr-summary",
        files: &[
            (
                "SKILL.md",
                include_str!("../../../bundled-skills/mando-pr-summary/SKILL.md"),
                false,
            ),
            (
                "fix-diagram.py",
                include_str!("../../../bundled-skills/mando-pr-summary/fix-diagram.py"),
                false,
            ),
        ],
    },
    BundledSkill {
        name: "mando-task",
        files: &[(
            "SKILL.md",
            include_str!("../../../bundled-skills/mando-task/SKILL.md"),
            false,
        )],
    },
];

fn claude_skills_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".claude").join("skills")
}

/// Write all bundled prod skills to `~/.claude/skills/mando-*`.
///
/// Overwrites existing files to keep skills in sync with the daemon version.
/// Skips if `~/.claude/skills/` cannot be created.
pub fn sync_bundled_skills() {
    let base = claude_skills_dir();
    if let Err(e) = fs::create_dir_all(&base) {
        warn!("cannot create {}: {e}", base.display());
        return;
    }

    let mut synced = 0u32;
    for skill in SKILLS {
        let dir = base.join(skill.name);
        if let Err(e) = fs::create_dir_all(&dir) {
            warn!("cannot create {}: {e}", dir.display());
            continue;
        }
        for &(filename, content, executable) in skill.files {
            let path = dir.join(filename);
            if write_if_changed(&path, content, executable) {
                synced += 1;
            }
        }
    }

    if synced > 0 {
        info!(synced, "synced bundled skills to {}", base.display());
    }
}

/// Write content to a file only if it differs from what's on disk.
/// Uses atomic write (temp file + rename) to prevent Claude Code from
/// reading a partially-written file during daemon restart.
/// Returns `true` if the file was written.
fn write_if_changed(path: &Path, content: &str, executable: bool) -> bool {
    let needs_write = match fs::read_to_string(path) {
        Ok(existing) => existing != content,
        Err(_) => true,
    };

    if needs_write {
        let tmp = path.with_extension("tmp");
        if let Err(e) = fs::write(&tmp, content) {
            warn!("cannot write {}: {e}", tmp.display());
            return false;
        }
        if let Err(e) = fs::rename(&tmp, path) {
            warn!("cannot rename {} -> {}: {e}", tmp.display(), path.display());
            let _ = fs::remove_file(&tmp);
            return false;
        }
    }

    // Always ensure executable bit, even if content is unchanged.
    if executable {
        let needs_chmod = fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 == 0)
            .unwrap_or(true);
        if needs_chmod {
            if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(0o755)) {
                warn!("cannot chmod {}: {e}", path.display());
            }
        }
    }

    needs_write
}
