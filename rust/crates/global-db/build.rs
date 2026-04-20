//! Build script for the `global-db` crate.
//!
//! Walks `migrations/*.sql` at build time and generates `$OUT_DIR/migrations.rs`,
//! which contains a `MIGRATIONS: &[(i64, &str)]` const with `include_str!` for
//! each file.  This replaces the hand-maintained array in `src/pool.rs`.
//!
//! The script also asserts that migration versions are unique, monotonically
//! increasing, and gap-free starting from 1.  Any filename that doesn't match
//! `<NNN>_<name>.sql` causes a build failure.
//!
//! PR #883 note: build scripts are compile-time utilities, not production
//! code. `panic!`, `unwrap`, and `expect` are the correct failure mode —
//! they abort the build with a visible error. The workspace-wide clippy
//! denies for those macros are relaxed here at file scope.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let migrations_dir = manifest_dir.join("migrations");

    // Rerun whenever the migrations directory or any SQL file changes.
    println!("cargo:rerun-if-changed={}", migrations_dir.display());

    let mut entries: Vec<(i64, String, PathBuf)> = Vec::new();

    if migrations_dir.is_dir() {
        let mut files: Vec<_> = fs::read_dir(&migrations_dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", migrations_dir.display()))
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file() && e.path().extension().and_then(|x| x.to_str()) == Some("sql")
            })
            .collect();

        // Sort by filename for deterministic ordering.
        files.sort_by_key(|e| e.file_name());

        for file_entry in &files {
            let path = file_entry.path();
            let name = file_entry.file_name().to_string_lossy().into_owned();

            // Parse version from filename prefix: `NNN_<rest>.sql`.
            let stem = name
                .strip_suffix(".sql")
                .unwrap_or_else(|| panic!("unexpected non-.sql file in migrations/: {name}"));
            let (num_str, _rest) = stem.split_once('_').unwrap_or_else(|| {
                panic!("migration filename must be <NNN>_<name>.sql, got: {name}")
            });
            let version: i64 = num_str.parse().unwrap_or_else(|_| {
                panic!("migration filename prefix must be numeric, got: {name}")
            });

            // Emit per-file rerun trigger.
            println!("cargo:rerun-if-changed={}", path.display());

            entries.push((version, name, path));
        }
    }

    // Validate: unique versions.
    let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for (version, name, _) in &entries {
        if !seen.insert(*version) {
            panic!("duplicate migration version {version} (file: {name})");
        }
    }

    // Validate: monotonic and gap-free starting at 1.
    let mut versions: Vec<i64> = entries.iter().map(|(v, _, _)| *v).collect();
    versions.sort_unstable();
    let expected: Vec<i64> = (1..=versions.len() as i64).collect();
    if versions != expected {
        panic!(
            "migration versions are not monotonically numbered starting at 1 with no gaps.\n\
             Expected: {expected:?}\n\
             Got:      {versions:?}"
        );
    }

    // Sort entries by version number for the generated array.
    entries.sort_by_key(|(v, _, _)| *v);

    // Emit $OUT_DIR/migrations.rs
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("migrations.rs");

    let mut code = String::new();
    code.push_str("pub(super) const MIGRATIONS: &[(i64, &str)] = &[\n");

    for (version, file_name, _path) in &entries {
        // Use CARGO_MANIFEST_DIR-relative path so the include_str! is absolute.
        code.push_str(&format!(
            "    ({version}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/migrations/{file_name}\"))),\n",
        ));
    }

    code.push_str("];\n");

    fs::write(&out_path, &code)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", out_path.display()));
}
