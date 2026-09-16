//! Access control for Telegram bot commands.
//!
//! Owner is defined by `TelegramConfig.owner` (ID or `"id|username"` pipe-delimited).
//! The bot only operates in DMs with the owner — group chats are rejected.

use settings::TelegramConfig;

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
