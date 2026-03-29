//! Access control for Telegram bot commands.
//!
//! Owner is defined by `TelegramConfig.owner` (ID or `"id|username"` pipe-delimited).
//! Group-safe commands are a small allow-list of read-only / info commands.

use mando_config::settings::TelegramConfig;

/// Commands allowed in group chats. Everything else is silently ignored.
const GROUP_SAFE: &[&str] = &[
    "start",
    "help",
    "status",
    "health",
    "ask", // captain/backlog
    "addlink",
    "research",
    "list",
    "simplelist",
    "saved",
    "scout", // scout
];

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

/// Check if `user_id` is on the allowlist (owner OR `allow_from` entries).
///
/// Used for group chats: if `allow_from` is non-empty, allow messages from
/// those user IDs too (not just the owner). If `allow_from` is empty, only
/// the owner is allowed.
pub fn is_allowed(config: &TelegramConfig, user_id: &str) -> bool {
    if is_owner(config, user_id) {
        return true;
    }
    if config.allow_from.is_empty() {
        return false;
    }
    // Check direct match
    if config.allow_from.iter().any(|id| id == user_id) {
        return true;
    }
    // Handle pipe-delimited sender IDs
    if user_id.contains('|') {
        return user_id
            .split('|')
            .filter(|p| !p.is_empty())
            .any(|part| config.allow_from.iter().any(|id| id == part));
    }
    false
}

/// Check if a command is safe to use in group chats.
pub fn is_group_safe(command: &str) -> bool {
    GROUP_SAFE.contains(&command)
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
    fn group_safe_known() {
        assert!(is_group_safe("start"));
        assert!(is_group_safe("help"));
        assert!(is_group_safe("status"));
        assert!(is_group_safe("health"));
        assert!(is_group_safe("ask"));
    }

    #[test]
    fn group_safe_rejected() {
        assert!(!is_group_safe("todo"));
        assert!(!is_group_safe("captain"));
        assert!(!is_group_safe("ops"));
        assert!(!is_group_safe("cancel"));
        assert!(!is_group_safe("input"));
    }

    // ── is_allowed tests ─────────────────────────────────────────────

    fn cfg_with_allowlist(owner: &str, allow_from: &[&str]) -> TelegramConfig {
        TelegramConfig {
            owner: owner.to_string(),
            allow_from: allow_from.iter().map(|s| s.to_string()).collect(),
            ..TelegramConfig::default()
        }
    }

    #[test]
    fn allowed_owner_always() {
        let c = cfg_with_allowlist("12345", &[]);
        assert!(is_allowed(&c, "12345"));
    }

    #[test]
    fn allowed_from_list() {
        let c = cfg_with_allowlist("12345", &["99999", "88888"]);
        assert!(is_allowed(&c, "99999"));
        assert!(is_allowed(&c, "88888"));
    }

    #[test]
    fn not_allowed_empty_list() {
        let c = cfg_with_allowlist("12345", &[]);
        assert!(!is_allowed(&c, "99999"));
    }

    #[test]
    fn allowed_pipe_delimited() {
        let c = cfg_with_allowlist("12345", &["99999"]);
        assert!(is_allowed(&c, "99999|bob"));
    }

    #[test]
    fn not_allowed_pipe_no_match() {
        let c = cfg_with_allowlist("12345", &["99999"]);
        assert!(!is_allowed(&c, "77777|bob"));
    }

    // ── Auto-registration scenario ──────────────────────────────────

    #[test]
    fn empty_owner_rejects_all() {
        // Before auto-registration, is_owner must reject everyone.
        assert!(!is_owner(&cfg(""), "12345"));
        assert!(!is_owner(&cfg(""), "99999"));
    }

    #[test]
    fn after_owner_set_accepts() {
        // Simulates what happens after auto_register_owner sets the field.
        let c = cfg("12345");
        assert!(is_owner(&c, "12345"));
        assert!(!is_owner(&c, "99999"));
    }
}
