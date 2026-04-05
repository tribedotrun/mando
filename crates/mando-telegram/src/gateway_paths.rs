//! Shared gateway route builders for Telegram flows.

use std::fmt::Display;

pub const TASKS: &str = "/api/tasks";
pub const TASKS_ADD: &str = "/api/tasks/add";
pub const TASKS_ACCEPT: &str = "/api/tasks/accept";
pub const TASKS_BULK: &str = "/api/tasks/bulk";
pub const TASKS_HANDOFF: &str = "/api/tasks/handoff";
pub const TASKS_REOPEN: &str = "/api/tasks/reopen";
pub const TASKS_REWORK: &str = "/api/tasks/rework";
pub const CAPTAIN_MERGE: &str = "/api/captain/merge";
pub const CAPTAIN_NUDGE: &str = "/api/captain/nudge";
pub const CAPTAIN_STOP: &str = "/api/captain/stop";
pub const CAPTAIN_TRIAGE: &str = "/api/captain/triage";
pub const SCOUT_ASK: &str = "/api/scout/ask";
pub const SCOUT_ITEMS: &str = "/api/scout/items";
pub const SCOUT_PROCESS: &str = "/api/scout/process";
pub const SCOUT_RESEARCH: &str = "/api/scout/research";

pub fn task_item(id: impl Display) -> String {
    format!("{TASKS}/{id}")
}

pub fn scout_item(id: impl Display) -> String {
    format!("{SCOUT_ITEMS}/{id}")
}

pub fn scout_article(id: impl Display) -> String {
    format!("{SCOUT_ITEMS}/{id}/article")
}

pub fn scout_telegraph(id: impl Display) -> String {
    format!("{SCOUT_ITEMS}/{id}/telegraph")
}

pub fn scout_act(id: impl Display) -> String {
    format!("{SCOUT_ITEMS}/{id}/act")
}

pub fn scout_items_with_status(status: Option<&str>, per_page: usize) -> String {
    match status.map(str::trim).filter(|status| !status.is_empty()) {
        Some(status) => format!("{SCOUT_ITEMS}?status={status}&per_page={per_page}"),
        None => format!("{SCOUT_ITEMS}?status=all&per_page={per_page}"),
    }
}

pub fn processed_scout_items(per_page: usize) -> String {
    format!("{SCOUT_ITEMS}?status=processed&per_page={per_page}")
}

#[cfg(test)]
mod tests {
    use super::{
        processed_scout_items, scout_act, scout_item, scout_items_with_status, scout_telegraph,
        task_item, TASKS,
    };

    #[test]
    fn builds_task_paths() {
        assert_eq!(TASKS, "/api/tasks");
        assert_eq!(task_item(42), "/api/tasks/42");
    }

    #[test]
    fn builds_scout_paths() {
        assert_eq!(scout_item(7), "/api/scout/items/7");
        assert_eq!(scout_act(7), "/api/scout/items/7/act");
        assert_eq!(scout_telegraph(7), "/api/scout/items/7/telegraph");
    }

    #[test]
    fn builds_status_list_paths() {
        assert_eq!(
            scout_items_with_status(None, 10000),
            "/api/scout/items?status=all&per_page=10000"
        );
        assert_eq!(
            scout_items_with_status(Some("saved"), 25),
            "/api/scout/items?status=saved&per_page=25"
        );
        assert_eq!(
            processed_scout_items(10000),
            "/api/scout/items?status=processed&per_page=10000"
        );
    }
}
