use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramStatus {
    pub enabled: bool,
    pub running: bool,
    pub owner: String,
    pub last_error: Option<String>,
    pub degraded: bool,
    pub restart_count: u32,
    pub mode: &'static str,
}
