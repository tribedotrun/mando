//! JSON file I/O and path sanitization utilities.

/// Sanitize an ID for safe use in file paths (prevent path traversal).
pub fn sanitize_path_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

/// Load a JSON file, returning `T::default()` on a missing file and an error
/// on any other failure (IO error or parse error).
pub fn load_json_file<T: serde::de::DeserializeOwned + Default>(
    path: &std::path::Path,
    _module: &str,
) -> Result<T, crate::json_error::SharedError> {
    use crate::json_error::SharedError;
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| SharedError::JsonParse {
            path: path.to_path_buf(),
            source: e,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(SharedError::Io {
            op: "read".into(),
            path: path.to_path_buf(),
            source: e,
        }),
    }
}

/// Save a value as pretty-printed JSON atomically.
pub fn save_json_file<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), crate::json_error::SharedError> {
    use crate::json_error::SharedError;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SharedError::Io {
            op: "create_dir_all".into(),
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let json = serde_json::to_string_pretty(value)?;

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!(
        "{}.tmp.{}.{}.{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or("json"),
        std::process::id(),
        seq,
        nanos,
    ));

    let mut f = std::fs::File::create(&tmp).map_err(|e| SharedError::Io {
        op: "create".into(),
        path: tmp.clone(),
        source: e,
    })?;
    f.write_all(json.as_bytes()).map_err(|e| SharedError::Io {
        op: "write".into(),
        path: tmp.clone(),
        source: e,
    })?;
    f.sync_all().map_err(|e| SharedError::Io {
        op: "fsync".into(),
        path: tmp.clone(),
        source: e,
    })?;
    drop(f);
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(SharedError::Io {
            op: "rename".into(),
            path: path.to_path_buf(),
            source: e,
        });
    }
    Ok(())
}
