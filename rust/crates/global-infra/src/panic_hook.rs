//! Global panic hook — enforces PR #883 invariant #1 (no silent crashes).
//!
//! Must be installed once, early in daemon startup, **after** the tracing
//! subscriber is ready. Routes panic message, location, and a backtrace
//! into the structured log before the process exits, so a crash never
//! disappears without leaving a JSONL record the operator can find.
//!
//! The hook is additive: it logs, then delegates to the previous hook so
//! the default Rust backtrace still prints to stderr for the interactive
//! terminal that spawned the daemon.

#![allow(
    clippy::print_stderr,
    reason = "the panic hook may need stderr when the structured logger itself has failed"
)]

use std::backtrace::Backtrace;
use std::panic::{self, PanicHookInfo};
use std::sync::Once;

static INSTALL: Once = Once::new();

/// Install the global panic hook. Safe to call more than once — only the
/// first call installs.
pub fn install() {
    INSTALL.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
            log_panic(info);
            previous(info);
        }));
    });
}

fn log_panic(info: &PanicHookInfo<'_>) {
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "unknown location".to_string());

    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<panic payload was not a string>".to_string());

    let backtrace = Backtrace::force_capture().to_string();

    tracing::error!(
        target: "mando.panic",
        module = "global-infra",
        location = %location,
        backtrace = %backtrace,
        "panic: {}",
        payload,
    );
}
