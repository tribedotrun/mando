//! Shared retry wrapper with exponential backoff + jitter for transient failures.

use std::time::Duration;

use tokio::time::sleep;

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the first).
    pub max_attempts: u32,
    /// Base delay between retries (doubled each attempt).
    pub base_delay: Duration,
    /// Maximum delay cap.
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
        }
    }
}

/// Whether an error is transient (retryable) or permanent.
pub enum RetryVerdict {
    Transient,
    Permanent,
}

/// Retry an async operation with exponential backoff + jitter.
///
/// `classify` inspects the error to decide if it's transient. Only transient
/// errors trigger a retry; permanent errors bail immediately.
pub async fn retry_on_transient<F, Fut, T, E, C>(
    config: &RetryConfig,
    mut classify: C,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    C: FnMut(&E) -> RetryVerdict,
    E: std::fmt::Display,
{
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if attempt >= config.max_attempts {
                    tracing::warn!(
                        module = "retry",
                        attempt = attempt,
                        max = config.max_attempts,
                        error = %e,
                        "all attempts exhausted"
                    );
                    return Err(e);
                }

                match classify(&e) {
                    RetryVerdict::Permanent => {
                        tracing::debug!(
                            module = "retry",
                            attempt = attempt,
                            error = %e,
                            "permanent error, not retrying"
                        );
                        return Err(e);
                    }
                    RetryVerdict::Transient => {
                        let delay = backoff_with_jitter(attempt - 1, config);
                        tracing::info!(
                            module = "retry",
                            attempt = attempt,
                            max = config.max_attempts,
                            delay_ms = delay.as_millis() as u64,
                            error = %e,
                            "transient error, retrying"
                        );
                        sleep(delay).await;
                    }
                }
            }
        }
    }
}

/// Compute backoff delay with jitter: `base * 2^attempt` capped at `max`, then
/// add random jitter of 0-50% of the delay.
fn backoff_with_jitter(attempt: u32, config: &RetryConfig) -> Duration {
    let base_ms = config.base_delay.as_millis() as u64;
    let max_ms = config.max_delay.as_millis() as u64;
    let delay_ms = base_ms.saturating_mul(1u64 << attempt).min(max_ms);

    // Simple jitter: add 0-50% of the delay using a cheap pseudo-random source.
    let jitter_ms = {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;
        nanos % (delay_ms / 2 + 1)
    };

    Duration::from_millis(delay_ms + jitter_ms)
}

/// Classify a CLI command failure as transient or permanent based on stderr.
pub fn classify_cli_error(stderr: &str) -> RetryVerdict {
    let lower = stderr.to_lowercase();
    if lower.contains("rate limit")
        || lower.contains("api rate")
        || lower.contains("secondary rate")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("network")
        || lower.contains("temporarily unavailable")
        || lower.contains("try again")
        || lower.contains("econnreset")
        || lower.contains("econnrefused")
        || lower.contains("etimedout")
    {
        RetryVerdict::Transient
    } else {
        RetryVerdict::Permanent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn succeeds_on_first_try() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let result: Result<&str, String> =
            retry_on_transient(&config, |_| RetryVerdict::Transient, || async { Ok("ok") }).await;
        assert_eq!(result.unwrap(), "ok");
    }

    #[tokio::test]
    async fn retries_transient_then_succeeds() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let counter = AtomicU32::new(0);
        let result: Result<&str, String> = retry_on_transient(
            &config,
            |_| RetryVerdict::Transient,
            || {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n < 2 {
                        Err("transient".into())
                    } else {
                        Ok("recovered")
                    }
                }
            },
        )
        .await;
        assert_eq!(result.unwrap(), "recovered");
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn permanent_error_stops_immediately() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let counter = AtomicU32::new(0);
        let result: Result<&str, String> = retry_on_transient(
            &config,
            |_| RetryVerdict::Permanent,
            || {
                counter.fetch_add(1, Ordering::SeqCst);
                async { Err("permanent".into()) }
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn exhausts_all_attempts() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
        };
        let counter = AtomicU32::new(0);
        let result: Result<&str, String> = retry_on_transient(
            &config,
            |_| RetryVerdict::Transient,
            || {
                counter.fetch_add(1, Ordering::SeqCst);
                async { Err("always fails".into()) }
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn classify_cli_transient() {
        assert!(matches!(
            classify_cli_error("API rate limit exceeded"),
            RetryVerdict::Transient
        ));
        assert!(matches!(
            classify_cli_error("HTTP 503 Service Unavailable"),
            RetryVerdict::Transient
        ));
        assert!(matches!(
            classify_cli_error("connection reset by peer"),
            RetryVerdict::Transient
        ));
    }

    #[test]
    fn classify_cli_permanent() {
        assert!(matches!(
            classify_cli_error("not found"),
            RetryVerdict::Permanent
        ));
        assert!(matches!(
            classify_cli_error("permission denied"),
            RetryVerdict::Permanent
        ));
    }
}
