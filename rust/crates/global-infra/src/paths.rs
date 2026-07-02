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

pub fn session_jsonl_dir() -> PathBuf {
    state_dir().join("session-jsonl")
}

pub fn session_jsonl_provider_dir(provider: &str) -> PathBuf {
    session_jsonl_dir().join(provider)
}

pub fn session_jsonl_path_for_provider(provider: &str, session_id: &str) -> PathBuf {
    session_jsonl_provider_dir(provider).join(format!("{session_id}.jsonl"))
}

pub fn codex_session_jsonl_path(session_id: &str) -> PathBuf {
    session_jsonl_path_for_provider("codex", session_id)
}

pub fn codex_derived_streams_dir() -> PathBuf {
    session_jsonl_provider_dir("codex-derived")
}

pub fn codex_derived_stream_path_for_session(session_id: &str) -> PathBuf {
    codex_derived_streams_dir().join(format!("{session_id}.jsonl"))
}

pub fn codex_derived_stream_meta_path_for_session(session_id: &str) -> PathBuf {
    codex_derived_streams_dir().join(format!("{session_id}.meta.json"))
}

pub fn opencode_streams_dir() -> PathBuf {
    session_jsonl_provider_dir("opencode")
}

pub fn opencode_stream_path_for_session(session_id: &str) -> PathBuf {
    opencode_streams_dir().join(format!("{session_id}.jsonl"))
}

pub fn opencode_stream_meta_path_for_session(session_id: &str) -> PathBuf {
    opencode_streams_dir().join(format!("{session_id}.meta.json"))
}

pub fn stream_path_for_session(session_id: &str) -> PathBuf {
    cc_streams_dir().join(format!("{session_id}.jsonl"))
}

pub fn stream_meta_path_for_session(session_id: &str) -> PathBuf {
    cc_streams_dir().join(format!("{session_id}.meta.json"))
}
