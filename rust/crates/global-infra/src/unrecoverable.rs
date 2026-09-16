//! `unrecoverable!` — the only sanctioned path to a deliberate crash.
//!
//! PR #883 bans bare `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!` on
//! daemon code paths. The replacement for "invariant violated, program
//! cannot continue" is this macro: it emits a structured `tracing::error!`
//! with the caller's message and context, then `panic!`s so stack unwinding
//! runs, `Drop` impls fire, buffered files flush, and the global panic hook
//! in `mando-gateway` lands a final structured log line. The tracing call
//! before the panic is a belt-and-suspenders guarantee for the case where
//! the panic hook is somehow disabled (e.g. early startup).
//!
//! We deliberately do **not** call `std::process::exit` — that skips Drop,
//! corrupting partially-written state. Every crash on the daemon side must
//! be a clean unwind.
//!
//! Usage:
//!
//! ```ignore
//! let cfg = load_config().unwrap_or_else(|e| {
//!     unrecoverable!("config load failed", e);
//! });
//! ```

/// Log a structured error and panic.
///
/// See the module docs for why this is not `std::process::exit`.
#[macro_export]
macro_rules! unrecoverable {
    ($msg:expr $(,)?) => {{
        ::tracing::error!(target: "mando.unrecoverable", module = "global-infra", message = $msg);
        ::std::panic!("unrecoverable: {}", $msg);
    }};
    ($msg:expr, $err:expr $(,)?) => {{
        let err_display = format!("{}", &$err);
        ::tracing::error!(
            target: "mando.unrecoverable",
            module = "global-infra",
            error = %err_display,
            message = $msg,
        );
        ::std::panic!("unrecoverable: {}: {}", $msg, err_display);
    }};
}
