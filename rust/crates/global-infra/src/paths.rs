use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    match std::env::var("HOME") {
        Ok(v) => PathBuf::from(v),
        Err(e) => crate::unrecoverable!("$HOME environment variable must be set", e),
    }
}

pub fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        home_dir().join(rest)
    } else if p == "~" {
        home_dir()
    } else {
        PathBuf::from(p)
    }
}

pub fn data_dir() -> PathBuf {
    if let Ok(v) = std::env::var("MANDO_DATA_DIR") {
        return expand_tilde(&v);
    }
    home_dir().join(".mando")
}

pub fn state_dir() -> PathBuf {
    data_dir().join("state")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn images_dir() -> PathBuf {
    data_dir().join("images")
}

pub fn bin_dir() -> PathBuf {
    data_dir().join("bin")
}

pub fn cc_streams_dir() -> PathBuf {
    state_dir().join("cc-streams")
}

pub fn stream_path_for_session(session_id: &str) -> PathBuf {
    cc_streams_dir().join(format!("{session_id}.jsonl"))
}

pub fn stream_meta_path_for_session(session_id: &str) -> PathBuf {
    cc_streams_dir().join(format!("{session_id}.meta.json"))
}
