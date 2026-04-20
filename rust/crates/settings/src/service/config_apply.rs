use crate::types::{ConfigApplyOutcome, ConfigChangeEvent};

pub fn build_config_apply_outcome(
    change: ConfigChangeEvent,
    publish_config_event: bool,
    publish_status_event: bool,
) -> ConfigApplyOutcome {
    ConfigApplyOutcome {
        change,
        reload_telegram: matches!(
            change,
            ConfigChangeEvent::Telegram | ConfigChangeEvent::Full
        ),
        publish_config_event,
        publish_status_event,
    }
}
