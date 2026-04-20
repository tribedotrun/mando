//! `best_effort!` — sanctioned replacement for `let _ = <Result-returning call>`.
//!
//! PR #883 invariant #2 (no silent error absorption) bans
//! `let _ = result_call()`. The replacement for genuinely best-effort code
//! paths — where the outer flow is already erroring out, or the callee's
//! failure has no actionable recovery — is this macro: it evaluates the
//! expression and routes any `Err` into a structured `tracing::debug!`
//! with the caller's context. The signal still lands in the JSONL log,
//! and the operator can grep for it during incident review.
//!
//! Usage:
//!
//! ```ignore
//! // Was: let _ = child.kill();
//! global_infra::best_effort!(child.kill(), "shell env probe teardown");
//! ```

/// Evaluate an expression, log any `Err` variant at `debug` level with the
/// supplied context message, and discard the value. See the module docs for
/// the full rationale.
#[macro_export]
macro_rules! best_effort {
    ($expr:expr, $ctx:expr $(,)?) => {{
        if let ::std::result::Result::Err(__be_err) = $expr {
            ::tracing::debug!(error = %__be_err, "best-effort failed: {}", $ctx);
        }
    }};
}

#[cfg(test)]
mod tests {
    #[test]
    fn accepts_ok_result_without_logging() {
        let ok: Result<(), std::io::Error> = Ok(());
        crate::best_effort!(ok, "ok path");
    }

    #[test]
    fn swallows_err_result() {
        let err: Result<(), std::io::Error> = Err(std::io::Error::other("expected in test"));
        crate::best_effort!(err, "err path");
    }
}
