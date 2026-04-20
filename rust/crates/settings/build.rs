//! Build script for the `settings` crate.
//!
//! Walks `bundled-skills/*/` at build time and generates `$OUT_DIR/bundled_skills.rs`,
//! which contains a `SKILLS: &[BundledSkill]` const with `include_str!` for each file.
//! This replaces the hand-maintained array in `src/config/skills.rs`.
//!
//! PR #883 note: build scripts are compile-time utilities, not production
//! code. `panic!`, `unwrap`, and `expect` are the correct failure mode —
//! they abort the build with a visible error. The workspace-wide clippy
//! denies for those macros are relaxed here at file scope.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    // CARGO_MANIFEST_DIR = rust/crates/settings; go up two levels to the Rust workspace root, then one more to the repo root.
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let rust_workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("could not resolve Rust workspace root from CARGO_MANIFEST_DIR");
    let repo_root = rust_workspace_root
        .parent()
        .expect("could not resolve repo root from Rust workspace root");
    let skills_dir = repo_root.join("bundled-skills");

    // Rerun whenever the directory tree changes.
    println!("cargo:rerun-if-changed={}", skills_dir.display());

    let mut skill_entries: Vec<(String, Vec<(String, PathBuf)>)> = Vec::new();

    if skills_dir.is_dir() {
        let mut dirs: Vec<_> = fs::read_dir(&skills_dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", skills_dir.display()))
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        // Deterministic ordering.
        dirs.sort_by_key(|e| e.file_name());

        for dir_entry in dirs {
            let skill_dir = dir_entry.path();
            let skill_name = dir_entry.file_name().to_string_lossy().into_owned();

            // Only process directories that contain SKILL.md.
            if !skill_dir.join("SKILL.md").exists() {
                continue;
            }

            // Emit rerun-if-changed for each individual file so that content
            // changes (without inode/mtime changes on the directory) also
            // trigger a rebuild.
            let mut files: Vec<_> = fs::read_dir(&skill_dir)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", skill_dir.display()))
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .collect();

            files.sort_by_key(|e| e.file_name());

            let mut file_entries: Vec<(String, PathBuf)> = Vec::new();
            for file_entry in files {
                let file_path = file_entry.path();
                let file_name = file_entry.file_name().to_string_lossy().into_owned();
                println!("cargo:rerun-if-changed={}", file_path.display());
                file_entries.push((file_name, file_path));
            }

            skill_entries.push((skill_name, file_entries));
        }
    }

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("bundled_skills.rs");

    let mut code = String::new();
    code.push_str("pub(super) const SKILLS: &[BundledSkill] = &[\n");

    for (skill_name, files) in &skill_entries {
        code.push_str("    BundledSkill {\n");
        code.push_str(&format!("        name: {:?},\n", skill_name));
        code.push_str("        files: &[\n");
        for (file_name, file_path) in files {
            // executable = true only if the file has the executable bit set.
            // We do NOT auto-set .py as executable; a file must be chmod +x in
            // the repo to be installed executable.  This preserves current
            // behavior (all bundled files are currently non-executable).
            let executable = is_executable(file_path);

            // Build a relative path from manifest_dir to the file so we can
            // emit concat!(env!("CARGO_MANIFEST_DIR"), "/relative/path").
            // manifest_dir = rust/crates/settings; repo_root = ../../..; so the
            // path will be like /../../../bundled-skills/<skill>/<file>.
            let rel = file_path.strip_prefix(repo_root).unwrap_or_else(|_| {
                panic!(
                    "file {} is not under repo root {}",
                    file_path.display(),
                    repo_root.display()
                )
            });
            // From manifest_dir to repo_root we need to go up three levels.
            let from_manifest = format!("/../../../{}", rel.display());

            code.push_str(&format!(
                "            ({file_name:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {from_manifest:?})), {executable}),\n",
            ));
        }
        code.push_str("        ],\n");
        code.push_str("    },\n");
    }

    code.push_str("];\n");

    fs::write(&out_path, &code)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", out_path.display()));
}

/// Returns true if the file has any executable bit set (owner, group, or other).
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
