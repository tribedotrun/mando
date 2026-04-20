use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UiDesiredState {
    #[default]
    Running,
    Suppressed,
    Updating,
}

#[derive(Clone)]
pub struct UiLaunchSpec {
    pub exec_path: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: HashMap<String, String>,
}

impl fmt::Debug for UiLaunchSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let env_keys: Vec<&str> = self.env.keys().map(String::as_str).collect();
        f.debug_struct("UiLaunchSpec")
            .field("exec_path", &self.exec_path)
            .field("args", &self.args)
            .field("cwd", &self.cwd)
            .field("env_keys", &env_keys)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiStatus {
    pub desired_state: UiDesiredState,
    pub current_pid: Option<i32>,
    pub launch_available: bool,
    pub running: bool,
    pub last_error: Option<String>,
    pub degraded: bool,
    pub restart_count: u32,
}

#[derive(Debug, Clone)]
pub struct UiAutoLaunchOptions {
    pub exec_path: String,
    pub entrypoint: String,
    pub gateway_port: u16,
    pub auth_token: String,
    pub headless: bool,
    pub app_mode: Option<String>,
    pub data_dir: Option<String>,
    pub log_dir: Option<String>,
    pub disable_security_warnings: Option<String>,
    pub inspect_port: Option<String>,
    pub cdp_port: Option<String>,
}
