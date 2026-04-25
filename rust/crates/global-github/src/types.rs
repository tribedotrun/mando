use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct PrStatus {
    pub number: String,
    pub author: String,
    pub ci_status: Option<String>,
    pub comments: i64,
    pub unresolved_threads: i64,
    pub unreplied_threads: i64,
    pub unaddressed_issue_comments: i64,
    pub body: String,
    pub head_sha: String,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeableStatus {
    Merged,
    Closed,
    Mergeable,
    Conflicted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrState {
    Open,
    Closed,
    Merged,
    Unknown(String),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PrComment {
    pub id: u64,
    #[serde(alias = "author", deserialize_with = "deserialize_author_lenient")]
    pub user: String,
    pub body: String,
    #[serde(alias = "createdAt")]
    pub created_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ThreadComment {
    pub author: String,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ReviewThread {
    pub id: String,
    pub is_resolved: bool,
    pub comments: Vec<ThreadComment>,
}

fn deserialize_author_lenient<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match val {
        Some(serde_json::Value::Object(map)) => map
            .get("login")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        Some(serde_json::Value::String(s)) => s,
        _ => String::new(),
    })
}
