//! Timeline event store — `~/.mando/state/timeline/{id}.json`.

use std::path::{Path, PathBuf};

use mando_shared::sanitize_path_id;

/// Compute the file path for an item's timeline.
pub(crate) fn timeline_path(state_dir: &Path, item_id: &str) -> PathBuf {
    state_dir
        .join("timeline")
        .join(format!("{}.json", sanitize_path_id(item_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_sanitized() {
        let p = timeline_path(Path::new("/tmp"), "../etc/passwd");
        assert!(!p.to_str().unwrap().contains(".."));
    }
}
