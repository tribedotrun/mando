#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigChangeEvent {
    None,
    Telegram,
    Captain,
    Ui,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRuntimeMode {
    Normal,
    Dev,
    Sandbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigApplyOutcome {
    pub change: ConfigChangeEvent,
    pub reload_telegram: bool,
    pub publish_config_event: bool,
    pub publish_status_event: bool,
}
