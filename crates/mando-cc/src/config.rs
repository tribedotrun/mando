//! Configuration for CC invocations.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Effort level for thinking depth.
#[derive(Debug, Clone, Copy)]
pub enum Effort {
    Low,
    Medium,
    High,
    Max,
}

impl Effort {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }
}

/// Thinking configuration.
#[derive(Debug, Clone)]
pub enum ThinkingConfig {
    Adaptive,
    Enabled { budget_tokens: u32 },
    Disabled,
}

/// Task budget — API-side token budget that lets the model pace itself.
///
/// Sent as `--task-budget` with the `task-budgets-2026-03-13` beta header.
#[derive(Debug, Clone, Copy)]
pub struct TaskBudget {
    pub total_tokens: u64,
}

/// Permission mode for tool execution.
#[derive(Debug, Clone)]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Plan,
    BypassPermissions,
    /// Auto-approve all tool uses without prompting (non-interactive).
    DontAsk,
    /// Auto mode — classifier decides permission level per tool use.
    Auto,
}

impl PermissionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::Plan => "plan",
            Self::BypassPermissions => "bypassPermissions",
            Self::DontAsk => "dontAsk",
            Self::Auto => "auto",
        }
    }
}

/// Configuration for a CC invocation.
#[derive(Clone)]
pub struct CcConfig {
    pub model: String,
    pub fallback_model: Option<String>,
    pub effort: Effort,
    pub thinking: Option<ThinkingConfig>,
    pub tools: Option<Vec<String>>,
    pub allowed_tools: Option<Vec<String>>,
    pub disallowed_tools: Option<Vec<String>>,
    pub permission_mode: Option<PermissionMode>,
    pub max_turns: Option<u32>,
    pub max_budget_usd: Option<f64>,
    pub json_schema: Option<serde_json::Value>,
    pub system_prompt: Option<String>,
    pub system_prompt_file: Option<PathBuf>,
    pub append_system_prompt: Option<String>,
    pub resume_session_id: Option<String>,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub settings: Option<String>,
    pub setting_sources: Option<Vec<String>>,
    pub mcp_config: Option<serde_json::Value>,
    pub task_budget: Option<TaskBudget>,
    pub betas: Vec<String>,
    pub timeout: Duration,
    pub caller: String,
    pub task_id: String,
    pub worker_name: String,
    pub project: String,
    pub extra_args: HashMap<String, Option<String>>,
    /// Hooks for the control protocol (PreToolUse, PostToolUse, etc.).
    pub hooks: Vec<crate::hooks::Hook>,
}

impl Default for CcConfig {
    fn default() -> Self {
        Self {
            model: "opus[1m]".into(),
            fallback_model: None,
            effort: Effort::Max,
            thinking: None,
            tools: None,
            allowed_tools: None,
            disallowed_tools: None,
            permission_mode: Some(PermissionMode::BypassPermissions),
            max_turns: None,
            max_budget_usd: None,
            json_schema: None,
            system_prompt: None,
            system_prompt_file: None,
            append_system_prompt: None,
            resume_session_id: None,
            session_id: None,
            cwd: PathBuf::new(),
            env: HashMap::new(),
            settings: None,
            setting_sources: None,
            mcp_config: None,
            task_budget: None,
            betas: Vec::new(),
            timeout: Duration::from_secs(120),
            caller: String::new(),
            task_id: String::new(),
            worker_name: String::new(),
            project: String::new(),
            extra_args: HashMap::new(),
            hooks: Vec::new(),
        }
    }
}

/// Builder for CcConfig.
#[derive(Default)]
pub struct CcConfigBuilder(CcConfig);

impl CcConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.0.model = model.into();
        self
    }
    pub fn fallback_model(mut self, m: impl Into<String>) -> Self {
        self.0.fallback_model = Some(m.into());
        self
    }
    pub fn effort(mut self, e: Effort) -> Self {
        self.0.effort = e;
        self
    }
    pub fn tools(mut self, t: Vec<String>) -> Self {
        self.0.tools = Some(t);
        self
    }
    pub fn allowed_tools(mut self, t: Vec<String>) -> Self {
        self.0.allowed_tools = Some(t);
        self
    }
    pub fn max_turns(mut self, n: u32) -> Self {
        self.0.max_turns = Some(n);
        self
    }
    pub fn max_budget_usd(mut self, b: f64) -> Self {
        self.0.max_budget_usd = Some(b);
        self
    }
    pub fn json_schema(mut self, schema: serde_json::Value) -> Self {
        self.0.json_schema = Some(schema);
        self
    }
    pub fn system_prompt(mut self, p: impl Into<String>) -> Self {
        self.0.system_prompt = Some(p.into());
        self
    }
    pub fn resume(mut self, session_id: impl Into<String>) -> Self {
        self.0.resume_session_id = Some(session_id.into());
        self
    }
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.0.session_id = Some(id.into());
        self
    }
    pub fn cwd(mut self, p: impl Into<PathBuf>) -> Self {
        self.0.cwd = p.into();
        self
    }
    pub fn env(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.0.env.insert(key.into(), val.into());
        self
    }
    pub fn task_budget(mut self, total_tokens: u64) -> Self {
        self.0.task_budget = Some(TaskBudget { total_tokens });
        // Auto-inject the required beta header.
        let beta = "task-budgets-2026-03-13".to_string();
        if !self.0.betas.contains(&beta) {
            self.0.betas.push(beta);
        }
        self
    }
    pub fn timeout(mut self, d: Duration) -> Self {
        self.0.timeout = d;
        self
    }
    pub fn caller(mut self, c: impl Into<String>) -> Self {
        self.0.caller = c.into();
        self
    }
    pub fn task_id(mut self, id: impl Into<String>) -> Self {
        self.0.task_id = id.into();
        self
    }
    pub fn worker_name(mut self, n: impl Into<String>) -> Self {
        self.0.worker_name = n.into();
        self
    }
    pub fn project(mut self, p: impl Into<String>) -> Self {
        self.0.project = p.into();
        self
    }
    pub fn build(self) -> CcConfig {
        self.0
    }
}

impl CcConfig {
    pub fn builder() -> CcConfigBuilder {
        CcConfigBuilder::new()
    }

    /// Build CLI arguments from this config.
    ///
    /// Does NOT include `-p` or the prompt — that goes via stdin in stream-json mode.
    pub fn to_cli_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // Always stream-json bidirectional.
        args.extend(["--input-format", "stream-json"].map(String::from));
        args.extend(["--output-format", "stream-json"].map(String::from));
        args.push("--verbose".into());

        // Effort.
        args.push("--effort".into());
        args.push(self.effort.as_str().into());

        // Model.
        if !self.model.is_empty() {
            args.push("--model".into());
            args.push(self.model.clone());
        }
        if let Some(ref fm) = self.fallback_model {
            args.push("--fallback-model".into());
            args.push(fm.clone());
        }

        // Thinking.
        if let Some(ref t) = self.thinking {
            match t {
                ThinkingConfig::Enabled { budget_tokens } => {
                    args.push("--max-thinking-tokens".into());
                    args.push(budget_tokens.to_string());
                }
                ThinkingConfig::Adaptive | ThinkingConfig::Disabled => {
                    // Adaptive is the default; disabled has no CLI flag (use effort=low).
                }
            }
        }

        // Tools.
        if let Some(ref tools) = self.tools {
            args.push("--tools".into());
            args.push(tools.join(","));
        }

        // Permissions.
        if let Some(ref pm) = self.permission_mode {
            args.push("--permission-mode".into());
            args.push(pm.as_str().into());
        }
        if let Some(ref at) = self.allowed_tools {
            for tool in at {
                args.push("--allowedTools".into());
                args.push(tool.clone());
            }
        }
        if let Some(ref dt) = self.disallowed_tools {
            for tool in dt {
                args.push("--disallowedTools".into());
                args.push(tool.clone());
            }
        }

        // Limits.
        if let Some(n) = self.max_turns {
            args.push("--max-turns".into());
            args.push(n.to_string());
        }
        if let Some(b) = self.max_budget_usd {
            args.push("--max-budget-usd".into());
            args.push(b.to_string());
        }

        // Structured output.
        if let Some(ref schema) = self.json_schema {
            args.push("--json-schema".into());
            args.push(schema.to_string());
        }

        // System prompt.
        if let Some(ref sp) = self.system_prompt {
            args.push("--system-prompt".into());
            args.push(sp.clone());
        }
        if let Some(ref spf) = self.system_prompt_file {
            args.push("--system-prompt-file".into());
            args.push(spf.display().to_string());
        }
        if let Some(ref asp) = self.append_system_prompt {
            args.push("--append-system-prompt".into());
            args.push(asp.clone());
        }

        // Session management.
        if let Some(ref rid) = self.resume_session_id {
            args.push("--resume".into());
            args.push(rid.clone());
        } else if let Some(ref sid) = self.session_id {
            args.push("--session-id".into());
            args.push(sid.clone());
        }
        // MCP.
        if let Some(ref mcp) = self.mcp_config {
            args.push("--mcp-config".into());
            args.push(mcp.to_string());
        }

        // Settings.
        if let Some(ref s) = self.settings {
            args.push("--settings".into());
            args.push(s.clone());
        }
        if let Some(ref ss) = self.setting_sources {
            args.push("--setting-sources".into());
            args.push(ss.join(","));
        }

        // Task budget.
        if let Some(ref tb) = self.task_budget {
            args.push("--task-budget".into());
            args.push(tb.total_tokens.to_string());
        }

        // Betas — collect configured betas plus any auto-injected ones.
        let mut betas = self.betas.clone();
        if self.task_budget.is_some() {
            let required = "task-budgets-2026-03-13".to_string();
            if !betas.contains(&required) {
                betas.push(required);
            }
        }
        if !betas.is_empty() {
            args.push("--betas".into());
            args.push(betas.join(","));
        }

        // Extra args (forward compatibility).
        for (flag, val) in &self.extra_args {
            args.push(format!("--{flag}"));
            if let Some(v) = val {
                args.push(v.clone());
            }
        }

        args
    }

    /// Effective session ID — reuse for resume, pre-assigned, or generate fresh.
    pub fn effective_session_id(&self) -> String {
        self.resume_session_id
            .clone()
            .or_else(|| self.session_id.clone())
            .unwrap_or_else(|| mando_uuid::Uuid::v4().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_args_include_all_config_options() {
        let config = CcConfig::builder()
            .model("claude-sonnet-4-6")
            .effort(Effort::High)
            .allowed_tools(vec!["Read".into(), "Grep".into()])
            .json_schema(serde_json::json!({"type": "object"}))
            .max_turns(10)
            .max_budget_usd(1.0)
            .timeout(Duration::from_secs(300))
            .caller("test")
            .build();

        let args = config.to_cli_args();

        assert!(args.contains(&"--input-format".to_string()));
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"--output-format".to_string()));

        let effort_idx = args.iter().position(|a| a == "--effort").unwrap();
        assert_eq!(args[effort_idx + 1], "high");

        let model_idx = args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(args[model_idx + 1], "claude-sonnet-4-6");

        let at_indices: Vec<_> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| *a == "--allowedTools")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(at_indices.len(), 2);

        assert!(args.contains(&"--json-schema".to_string()));
        assert!(args.contains(&"--max-turns".to_string()));
        assert!(args.contains(&"--max-budget-usd".to_string()));
    }
}
