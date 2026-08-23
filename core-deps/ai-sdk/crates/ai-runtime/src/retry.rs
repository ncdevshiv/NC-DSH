//! Retry policies and execution with exponential backoff + jitter.

use std::future::Future;
use std::time::Duration;

use rand::Rng;
use tracing::{debug, warn};

use ai_errors::{AiError, TimeoutError};

use crate::Result;

/// Whether an error should be retried. Defaults to [`AiError::is_retryable`]
/// (rate limits, timeouts, network failures, provider 5xx).
pub type RetryableFn = fn(&AiError) -> bool;

/// Configuration for retrying fallible operations.
///
/// `max_attempts` is the total number of attempts (1 = no retries).
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    /// Base delay for the first backoff.
    pub base_delay: Duration,
    /// Upper bound for any single delay.
    pub max_delay: Duration,
    /// Jitter as a fraction of the computed delay (`0.0` = none, `1.0` = ±100%).
    pub jitter: f64,
    /// Classifier deciding whether a given error should be retried.
    pub retryable: RetryableFn,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(30),
            jitter: 0.2,
            retryable: AiError::is_retryable,
        }
    }
}

impl RetryPolicy {
    /// A policy with no retries (single attempt).
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            ..Default::default()
        }
    }

    pub fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts.max(1);
        self
    }

    pub fn with_base_delay(mut self, base_delay: Duration) -> Self {
        self.base_delay = base_delay;
        self
    }

    pub fn with_max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = jitter.clamp(0.0, 1.0);
        self
    }

    pub fn with_retryable(mut self, retryable: RetryableFn) -> Self {
        self.retryable = retryable;
        self
    }
}

/// Computes the delay before retry attempt `attempt` (0-based) for `policy`,
/// including exponential backoff and ±jitter.
pub fn backoff_delay(attempt: u32, policy: &RetryPolicy) -> Duration {
    let exp = policy.base_delay.as_millis() as f64 * 2f64.powi(attempt as i32);
    let capped = exp.min(policy.max_delay.as_millis() as f64);
    let jitter_amount = capped * policy.jitter;
    let jittered = if policy.jitter > 0.0 {
        // Real randomness per call: deriving the fraction from the attempt
        // number alone handed every caller the identical backoff sequence
        // (thundering herd). `jitter = 0.0` remains fully deterministic for
        // tests.
        let fraction: f64 = rand::thread_rng().gen_range(-1.0..1.0);
        capped + jitter_amount * fraction
    } else {
        capped
    };
    Duration::from_millis(jittered.max(0.0) as u64)
}

/// Runs `op`, retrying on retryable errors according to `policy`.
///
/// `op` is invoked fresh for each attempt (closures that construct the
/// request are the intended usage). The last error is returned when all
/// attempts are exhausted. Retries are logged at `WARN`; per-attempt details
/// at `DEBUG`.
pub async fn retry<F, Fut, T>(policy: &RetryPolicy, operation: &str, mut op: F) -> Result<T>
where
    F: FnMut() -> Fut + Send,
    Fut: Future<Output = Result<T>> + Send,
{
    let mut last_error: Option<AiError> = None;

    for attempt in 0..policy.max_attempts {
        if attempt > 0 {
            // Prefer the server-provided `Retry-After` delay when the last
            // error was a rate limit that carried one; otherwise use the
            // configured exponential backoff (OpenAI SDKs ignore
            // `Retry-After` — honoring it avoids hammering a throttled
            // endpoint).
            let delay = match last_error.as_ref() {
                Some(AiError::RateLimit(rate_limit)) => rate_limit
                    .retry_after
                    .unwrap_or_else(|| backoff_delay(attempt - 1, policy)),
                _ => backoff_delay(attempt - 1, policy),
            };
            debug!(
                operation,
                attempt,
                delay_ms = delay.as_millis(),
                "retrying after backoff"
            );
            tokio::time::sleep(delay).await;
        }

        match op().await {
            Ok(value) => {
                if attempt > 0 {
                    debug!(operation, attempt, "operation succeeded after retry");
                }
                return Ok(value);
            }
            Err(err) => {
                let retryable = (policy.retryable)(&err);
                let exhausted = attempt + 1 >= policy.max_attempts;
                if retryable && !exhausted {
                    warn!(
                        operation,
                        attempt = attempt + 1,
                        max_attempts = policy.max_attempts,
                        error = %err,
                        "transient failure, will retry"
                    );
                } else {
                    if exhausted && retryable {
                        warn!(operation, max_attempts = policy.max_attempts, error = %err, "retries exhausted");
                    }
                    return Err(err);
                }
                last_error = Some(err);
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| AiError::Timeout(TimeoutError::new(operation, Duration::from_secs(0)))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use ai_errors::{NetworkError, RateLimitError};

    #[test]
    fn backoff_grows_exponentially_and_caps() {
        let policy = RetryPolicy::default().with_jitter(0.0);
        let d0 = backoff_delay(0, &policy);
        let d1 = backoff_delay(1, &policy);
        let d2 = backoff_delay(2, &policy);
        assert!(d1 > d0, "{d1:?} should exceed {d0:?}");
        assert!(d2 > d1, "{d2:?} should exceed {d1:?}");
        assert_eq!(d0, Duration::from_millis(200));
        assert_eq!(d1, Duration::from_millis(400));
        assert_eq!(d2, Duration::from_millis(800));
    }

    #[test]
    fn backoff_respects_max_delay() {
        let policy = RetryPolicy::default()
            .with_base_delay(Duration::from_secs(5))
            .with_max_delay(Duration::from_secs(9))
            .with_jitter(0.0);
        let d = backoff_delay(4, &policy);
        assert_eq!(d, Duration::from_secs(9));
    }

    #[test]
    fn jittered_delays_vary_and_respect_bounds() {
        let policy = RetryPolicy::default()
            .with_base_delay(Duration::from_millis(400))
            .with_jitter(0.5);
        let first = backoff_delay(0, &policy).as_millis();
        let mut varied = false;
        for _ in 0..200 {
            let millis = backoff_delay(0, &policy).as_millis();
            // ±50% around the 400 ms capped delay.
            assert!(
                (200..=600).contains(&millis),
                "jittered delay {millis} ms outside bounds"
            );
            varied |= millis != first;
        }
        assert!(
            varied,
            "identical delays across 200 samples: jitter is not random"
        );
    }

    #[tokio::test]
    async fn retry_succeeds_after_transient_failures() {
        let calls = Arc::new(AtomicU32::new(0));
        let policy = RetryPolicy::default()
            .with_max_attempts(4)
            .with_base_delay(Duration::from_millis(1))
            .with_jitter(0.0);
        let calls_clone = calls.clone();
        let result: Result<u32> = retry(
            &policy,
            "test-op",
            move || -> std::future::Ready<Result<u32>> {
                let calls = calls_clone.clone();
                let n = calls.fetch_add(1, Ordering::SeqCst);
                std::future::ready(if n < 2 {
                    Err(AiError::Network(NetworkError::new("test-op", "flaky")))
                } else {
                    Ok(n)
                })
            },
        )
        .await;
        assert_eq!(result.unwrap(), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_gives_up_and_returns_last_error() {
        let policy = RetryPolicy::default()
            .with_max_attempts(3)
            .with_base_delay(Duration::from_millis(1))
            .with_jitter(0.0);
        let result: Result<i32> =
            retry(&policy, "test-op", || -> std::future::Ready<Result<i32>> {
                std::future::ready(Err(AiError::Network(NetworkError::new(
                    "test-op",
                    "always down",
                ))))
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn non_retryable_errors_are_not_retried() {
        let calls = Arc::new(AtomicU32::new(0));
        let policy = RetryPolicy::default()
            .with_max_attempts(5)
            .with_base_delay(Duration::from_millis(1));
        let calls_clone = calls.clone();
        let result: Result<i32> = retry(
            &policy,
            "test-op",
            move || -> std::future::Ready<Result<i32>> {
                calls_clone.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Err(AiError::Validation(ai_errors::ValidationError::new(
                    "bad input",
                ))))
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "validation errors must not retry"
        );
    }

    #[tokio::test]
    async fn rate_limit_retry_after_is_honored() {
        let policy = RetryPolicy::default()
            .with_max_attempts(3)
            .with_base_delay(Duration::from_secs(10)) // long backoff: must NOT be used
            .with_jitter(0.0);
        let started = std::time::Instant::now();
        let result: Result<i32> = retry(&policy, "test-op", || {
            std::future::ready(Err(AiError::RateLimit(
                RateLimitError::new("openai", "slow down")
                    .with_retry_after(Duration::from_millis(30)),
            )))
        })
        .await;
        let elapsed = started.elapsed();
        assert!(result.is_err());
        // Two waits happened: each ~30 ms (Retry-After), far below the 10 s
        // backoff — proving the server-provided delay was honored.
        assert!(
            elapsed < Duration::from_millis(1000),
            "retry used Retry-After (elapsed {elapsed:?})"
        );
        assert!(
            elapsed >= Duration::from_millis(55),
            "at least two waits happened"
        );
    }
}
