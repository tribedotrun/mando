//! Runtime orchestration — composes biz + io.
//!
//! All functions are async.

pub mod captain_merge;
pub mod captain_review;
mod captain_review_verdict;
pub mod ci_gate;
pub mod clarifier;
mod clarifier_session;
mod clarifier_validate;
pub mod dashboard;
pub mod dashboard_timeline;
pub mod dashboard_triage;
pub mod dispatch_phase;
pub mod distiller;
pub mod guardian;
pub mod linear_integration;
pub mod mergeability;
mod mergeability_rebase;
mod mergeability_review;
pub mod notify;
pub mod reconciler;
pub mod review_phase;
mod session_reconcile;
pub mod spawn_phase;
pub mod spawn_phase_review;
pub mod spawner;
pub mod spawner_lifecycle;
pub mod task_ask;
mod task_notes;
pub mod tick;
mod tick_action_loop;
mod tick_classify;
mod tick_guard;
pub mod tick_journal;
pub mod tick_persist;
mod tick_post;
pub mod tick_spawn;
pub mod timeline_backfill;
pub mod timeline_emit;
