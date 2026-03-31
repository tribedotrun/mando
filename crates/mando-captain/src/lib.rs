//! mando-captain — captain tick loop, task engine, worker management,
//! and the deterministic state machine.
//!
//! Layer discipline:
//! - `biz/`     — pure functions, no I/O
//! - `io/`      — thin async wrappers around external systems (pub(crate))
//! - `runtime/` — orchestration: composes biz + io

// All io/ functions are now wired (issue #4).

pub mod biz;
pub mod io;
pub(crate) mod pr_evidence;
pub mod runtime;
