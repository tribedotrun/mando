//! Access control for Telegram bot commands.
//!
//! Owner is defined by `TelegramConfig.owner` (ID or `"id|username"` pipe-delimited).
//! The bot only operates in DMs with the owner — group chats are rejected.

use mando_config::settings::TelegramConfig;

/// Check if `user_id` matches the configured owner.
///
/// Returns `false` when owner is empty — callers should handle the
/// no-owner-yet case before calling this (see `bot.rs` auto-registration).
///
/// Handles pipe-delimited format: if `user_id` is `"12345|username"`,
/// it checks each part against the owner field.
pub fn is_owner(config: &TelegramConfig, user_id: &str) -> bool {
    let owner = &config.owner;
    if owner.is_empty() {
        return false;
    }
    if user_id == owner {
        return true;
    }
    // Handle pipe-delimited sender IDs (e.g. "12345|username")
    if user_id.contains('|') {
        return user_id
            .split('|')
            .any(|part| !part.is_empty() && part == owner);
    }
    false
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(owner: &str) -> TelegramConfig {
        TelegramConfig {
            owner: owner.to_string(),
            ..TelegramConfig::default()
        }
    }

    #[test]
    fn owner_exact_match() {
        assert!(is_owner(&cfg("12345"), "12345"));
    }

    #[test]
    fn owner_no_match() {
        assert!(!is_owner(&cfg("12345"), "99999"));
    }

    #[test]
    fn owner_empty_config() {
        assert!(!is_owner(&cfg(""), "12345"));
    }

    #[test]
    fn owner_pipe_delimited_match() {
        assert!(is_owner(&cfg("12345"), "12345|testuser"));
    }

    #[test]
    fn owner_pipe_delimited_username_match() {
        assert!(is_owner(&cfg("testuser"), "12345|testuser"));
    }

    #[test]
    fn owner_pipe_delimited_no_match() {
        assert!(!is_owner(&cfg("99999"), "12345|testuser"));
    }

    #[test]
    fn empty_owner_rejects_all() {
        assert!(!is_owner(&cfg(""), "12345"));
        assert!(!is_owner(&cfg(""), "99999"));
    }

    #[test]
    fn after_owner_set_accepts() {
        let c = cfg("12345");
        assert!(is_owner(&c, "12345"));
        assert!(!is_owner(&c, "99999"));
    }
}
