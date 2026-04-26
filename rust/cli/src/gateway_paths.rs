//! Shared daemon route builders for CLI flows.

use std::fmt::Display;

pub const CAPTAIN_ADOPT: &str = "/api/captain/adopt";
pub const CAPTAIN_NUDGE: &str = "/api/captain/nudge";
pub const CAPTAIN_STOP: &str = "/api/captain/stop";
pub const CAPTAIN_TICK: &str = "/api/captain/tick";
pub const CAPTAIN_TRIAGE: &str = "/api/captain/triage";
pub const CHANNELS: &str = "/api/channels";
pub const CREDENTIALS: &str = "/api/credentials";
pub const CREDENTIALS_PICK: &str = "/api/credentials/pick";
pub const FIRECRAWL_SCRAPE: &str = "/api/firecrawl/scrape";
pub const HEALTH: &str = "/api/health";
pub const HEALTH_SYSTEM: &str = "/api/health/system";
pub const NOTIFY: &str = "/api/notify";
pub const PROJECTS: &str = "/api/projects";
pub const SCOUT_ASK: &str = "/api/scout/ask";
pub const SCOUT_BULK: &str = "/api/scout/bulk";
pub const SCOUT_BULK_DELETE: &str = "/api/scout/bulk-delete";
pub const SCOUT_ITEMS: &str = "/api/scout/items";
pub const SCOUT_RESEARCH: &str = "/api/scout/research";
pub const SESSIONS: &str = "/api/sessions";
pub const TASKS: &str = "/api/tasks";
pub const TASKS_ACCEPT: &str = "/api/tasks/accept";
pub const TASKS_ADD: &str = "/api/tasks/add";
pub const TASKS_ASK: &str = "/api/tasks/ask";
pub const TASKS_ASK_END: &str = "/api/tasks/ask/end";
pub const TASKS_DELETE: &str = "/api/tasks/delete";
pub const TASKS_HANDOFF: &str = "/api/tasks/handoff";
pub const TASKS_MERGE: &str = "/api/tasks/merge";
pub const TASKS_QUEUE: &str = "/api/tasks/queue";
pub const TASKS_REOPEN: &str = "/api/tasks/reopen";
pub const TASKS_RETRY: &str = "/api/tasks/retry";
pub const TASKS_REWORK: &str = "/api/tasks/rework";
pub const TASKS_STOP: &str = "/api/tasks/stop";
pub const TASKS_WITH_ARCHIVED: &str = "/api/tasks?include_archived=true";
pub const UI_LAUNCH: &str = "/api/ui/launch";
pub const WORKTREES: &str = "/api/worktrees";
pub const WORKTREES_CLEANUP: &str = "/api/worktrees/cleanup";
pub const WORKTREES_PRUNE: &str = "/api/worktrees/prune";
pub const WORKTREES_REMOVE: &str = "/api/worktrees/remove";

pub fn project(encoded: impl Display) -> String {
    format!("{PROJECTS}/{encoded}")
}

pub fn scout_item(id: impl Display) -> String {
    format!("{SCOUT_ITEMS}/{id}")
}

pub fn scout_article(id: impl Display) -> String {
    format!("{SCOUT_ITEMS}/{id}/article")
}

pub fn scout_act(id: impl Display) -> String {
    format!("{SCOUT_ITEMS}/{id}/act")
}

pub fn scout_items_query(query: impl Display) -> String {
    format!("{SCOUT_ITEMS}?{query}")
}

pub fn scout_research_run(run_id: impl Display) -> String {
    format!("{SCOUT_RESEARCH}/{run_id}")
}

pub fn scout_sessions(id: impl Display) -> String {
    format!("{SCOUT_ITEMS}/{id}/sessions")
}

pub fn scout_telegraph(id: impl Display) -> String {
    format!("{SCOUT_ITEMS}/{id}/telegraph")
}

pub fn sessions_query(query: impl Display) -> String {
    format!("{SESSIONS}?{query}")
}

pub fn session_cost(session_id: impl Display) -> String {
    format!("{SESSIONS}/{session_id}/cost")
}

pub fn session_messages(session_id: impl Display) -> String {
    format!("{SESSIONS}/{session_id}/messages")
}

pub fn session_messages_limit(session_id: impl Display, limit: impl Display) -> String {
    format!("{}/messages?limit={limit}", session_base(session_id))
}

pub fn session_stream(session_id: impl Display) -> String {
    format!("{SESSIONS}/{session_id}/stream")
}

pub fn session_stream_types(session_id: impl Display, types: impl Display) -> String {
    format!("{}/stream?types={types}", session_base(session_id))
}

pub fn session_tools(session_id: impl Display) -> String {
    format!("{SESSIONS}/{session_id}/tools")
}

pub fn session_events(session_id: impl Display) -> String {
    format!("{SESSIONS}/{session_id}/events")
}

pub fn task_clarify(id: impl Display) -> String {
    format!("{TASKS}/{id}/clarify")
}

pub fn task_evidence(id: impl Display) -> String {
    format!("{TASKS}/{id}/evidence")
}

pub fn task_history(id: impl Display) -> String {
    format!("{TASKS}/{id}/history")
}

pub fn task_item(id: impl Display) -> String {
    format!("{TASKS}/{id}")
}

pub fn task_sessions(id: impl Display) -> String {
    format!("{TASKS}/{id}/sessions")
}

pub fn task_sessions_caller(id: impl Display, caller: impl Display) -> String {
    format!("{}/sessions?caller={caller}", task_base(id))
}

pub fn task_summary(id: impl Display) -> String {
    format!("{TASKS}/{id}/summary")
}

pub fn task_timeline(id: impl Display) -> String {
    format!("{TASKS}/{id}/timeline")
}

pub fn task_timeline_last(id: impl Display, last: impl Display) -> String {
    format!("{}/timeline?last={last}", task_base(id))
}

fn session_base(session_id: impl Display) -> String {
    format!("{SESSIONS}/{session_id}")
}

fn task_base(id: impl Display) -> String {
    format!("{TASKS}/{id}")
}
