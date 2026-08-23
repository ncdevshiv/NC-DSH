//! Circuit breaker: fail fast when a dependency is unhealthy.

use std::future::Future;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use ai_errors::{AiError, ProviderError};

use crate::Result;

/// The breaker's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation; failures are counted.
    Closed,
    /// Too many failures; calls fail fast without hitting the dependency.
    Open,
    /// Probing: a limited number of calls are allowed to test recovery.
    HalfOpen,
}

/// State machine storage with timestamps.
struct Inner {
    /// Failure timestamps within the current window (only used when closed).
    failures: Vec<Instant>,
    /// When the circuit was opened (for recovery timeout).
    opened_at: Option<Instant>,
    /// Remaining half-open probes.
    half_open_permits: u32,
}

/// A circuit breaker with configurable failure threshold, window, recovery
/// timeout, and half-open probing.
///
/// - **Closed**: calls execute; failures within the window are counted.
///   When the threshold is reached the circuit opens.
/// - **Open**: calls fail fast with a [`ProviderError`] until the recovery
///   timeout elapses.
/// - **HalfOpen**: a bounded number of probe calls execute; a success
///   closes the circuit, a failure re-opens it.
///
/// The breaker is cheap and lock-free for the hot paths (atomic state read);
/// state transitions take a short mutex.
pub struct CircuitBreaker {
    state: AtomicU8,
    failure_threshold: u32,
    window: Duration,
    recovery_timeout: Duration,
    half_open_max: u32,
    inner: Mutex<Inner>,
}

/// Bookkeeping for one in-flight [`CircuitBreaker::execute`] probe.
///
/// Dropping the guard restores a consumed half-open permit unless the probe
/// already reached a recorded verdict. Without this, a caller abandoning
/// `execute` mid-operation (timeout / cancellation race) leaves HalfOpen at
/// zero permits — the breaker then fails fast forever, because neither
/// `record_success` nor `record_failure` ever runs and only a state
/// transition re-arms permits.
struct ProbeGuard<'a> {
    breaker: &'a CircuitBreaker,
    /// Set once the probe's outcome was recorded (success, or a failure
    /// routed through `record_failure`). When still false on drop, the probe
    /// produced no verdict — it was cancelled or ended with a non-retryable
    /// error — and its permit must be returned.
    settled: bool,
}

impl ProbeGuard<'_> {
    fn settle(&mut self) {
        self.settled = true;
    }
}

impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        // Same lock discipline as `try_acquire` / `record_*`. Restoration is
        // skipped unless the breaker is still HalfOpen *and* below the cap:
        // when a recorded failure already transitioned HalfOpen → Open (the
        // Open path re-arms fresh permits later), or a sibling probe closed
        // the circuit, permit bookkeeping was reset and must not be
        // corrupted by this late restore.
        let mut inner = self.breaker.inner.lock();
        if self.breaker.state() == CircuitState::HalfOpen
            && inner.half_open_permits < self.breaker.half_open_max
        {
            inner.half_open_permits += 1;
        }
    }
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("state", &self.state())
            .field("failure_threshold", &self.failure_threshold)
            .field("recovery_timeout", &self.recovery_timeout)
            .finish()
    }
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, window: Duration, recovery_timeout: Duration) -> Self {
        Self {
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_threshold: failure_threshold.max(1),
            window,
            recovery_timeout,
            half_open_max: 1,
            inner: Mutex::new(Inner {
                failures: Vec::new(),
                opened_at: None,
                half_open_permits: 0,
            }),
        }
    }

    /// A default breaker: 5 failures within 5 minutes, 30 s recovery.
    pub fn defaults() -> Self {
        Self::new(5, Duration::from_secs(300), Duration::from_secs(30))
    }

    pub fn state(&self) -> CircuitState {
        let s = self.state.load(Ordering::Acquire);
        match s {
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }

    /// Remaining half-open probes (test introspection).
    #[cfg(test)]
    fn available_half_open_permits(&self) -> u32 {
        self.inner.lock().half_open_permits
    }

    /// Whether a call may proceed. When the circuit is open, fails fast.
    fn try_acquire(&self, dependency: &str) -> Result<()> {
        match self.state() {
            CircuitState::Closed => Ok(()),
            CircuitState::HalfOpen => {
                let mut inner = self.inner.lock();
                if inner.half_open_permits > 0 {
                    inner.half_open_permits -= 1;
                    Ok(())
                } else {
                    Err(self.fast_fail(dependency))
                }
            }
            CircuitState::Open => {
                // Re-evaluate the recovery timeout under the lock.
                let mut inner = self.inner.lock();
                let elapsed = inner.opened_at.map(|t| t.elapsed()).unwrap_or_default();
                if elapsed >= self.recovery_timeout {
                    // Transition to half-open with fresh probe permits.
                    self.state
                        .store(CircuitState::HalfOpen as u8, Ordering::Release);
                    inner.half_open_permits = self.half_open_max;
                    inner.failures.clear();
                    if inner.half_open_permits > 0 {
                        inner.half_open_permits -= 1;
                        Ok(())
                    } else {
                        Err(self.fast_fail(dependency))
                    }
                } else {
                    Err(self.fast_fail(dependency))
                }
            }
        }
    }

    fn fast_fail(&self, dependency: &str) -> AiError {
        AiError::Provider(ProviderError::new(
            dependency,
            format!(
                "circuit is {:?}; dependency presumed unhealthy ({} failures, {} recovery)",
                self.state(),
                self.failure_threshold,
                self.recovery_timeout.as_secs()
            ),
        ))
    }

    /// Records a successful call.
    fn record_success(&self) {
        match self.state() {
            CircuitState::Closed => {
                // Optionally prune old failures; simplest is to clear on
                // success in closed state — a healthy dependency resets the
                // window.
                let mut inner = self.inner.lock();
                inner.failures.clear();
            }
            CircuitState::HalfOpen => {
                // A successful probe closes the circuit.
                self.state
                    .store(CircuitState::Closed as u8, Ordering::Release);
                let mut inner = self.inner.lock();
                inner.failures.clear();
                inner.opened_at = None;
            }
            CircuitState::Open => {}
        }
    }

    /// Records a failure; opens the circuit when the threshold is exceeded.
    fn record_failure(&self) {
        match self.state() {
            CircuitState::Closed => {
                let mut inner = self.inner.lock();
                let now = Instant::now();
                inner
                    .failures
                    .retain(|t| now.duration_since(*t) <= self.window);
                inner.failures.push(now);
                if inner.failures.len() >= self.failure_threshold as usize {
                    self.state
                        .store(CircuitState::Open as u8, Ordering::Release);
                    inner.opened_at = Some(now);
                }
            }
            CircuitState::HalfOpen => {
                // A failed probe re-opens the circuit immediately.
                self.state
                    .store(CircuitState::Open as u8, Ordering::Release);
                let mut inner = self.inner.lock();
                inner.opened_at = Some(Instant::now());
            }
            CircuitState::Open => {}
        }
    }

    /// Runs `operation` through the circuit. While open, fails fast without
    /// invoking the operation.
    ///
    /// Cancellation-safe: dropping the returned future mid-operation (the
    /// timeout / cancellation race) restores any consumed half-open probe
    /// permit via [`ProbeGuard`], so an abandoned probe can never strand the
    /// breaker in HalfOpen with zero permits.
    pub async fn execute<T, F, Fut>(&self, dependency: &str, operation: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        self.try_acquire(dependency)?;
        let mut guard = ProbeGuard {
            breaker: self,
            settled: false,
        };
        let outcome = match operation().await {
            Ok(value) => {
                guard.settle();
                self.record_success();
                Ok(value)
            }
            Err(err) => {
                if err.is_retryable() || matches!(err, AiError::Timeout(_)) {
                    guard.settle();
                    self.record_failure();
                }
                Err(err)
            }
        };
        // No-op when a verdict was recorded above; otherwise restores the
        // half-open permit (cancelled probe, or non-retryable probe error).
        drop(guard);
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    async fn succeed() -> Result<i32> {
        Ok(1)
    }

    async fn fail() -> Result<i32> {
        Err(AiError::Provider(
            ProviderError::new("dep", "boom").with_status(500),
        ))
    }

    #[tokio::test]
    async fn opens_after_threshold_and_fails_fast() {
        let breaker = CircuitBreaker::new(3, Duration::from_secs(60), Duration::from_secs(3600));
        for _ in 0..3 {
            let _ = breaker.execute("dep", fail).await;
        }
        assert_eq!(breaker.state(), CircuitState::Open);
        // Fails fast without calling the operation.
        let calls = AtomicU32::new(0);
        let err = breaker
            .execute("dep", || {
                calls.fetch_add(1, Ordering::SeqCst);
                succeed()
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("circuit"), "{err}");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn success_in_closed_state_resets_failures() {
        let breaker = CircuitBreaker::new(3, Duration::from_secs(60), Duration::from_secs(3600));
        let _ = breaker.execute("dep", fail).await;
        let _ = breaker.execute("dep", fail).await;
        let _ = breaker.execute("dep", succeed).await;
        // Still closed; a third failure should not open it (window reset).
        let _ = breaker.execute("dep", fail).await;
        let _ = breaker.execute("dep", fail).await;
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn recovers_to_half_open_then_closed() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(60), Duration::from_millis(50));
        let _ = breaker.execute("dep", fail).await;
        let _ = breaker.execute("dep", fail).await;
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait past recovery timeout, then probe succeeds → closed.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let value = breaker.execute("dep", succeed).await.unwrap();
        assert_eq!(value, 1);
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn failed_probe_reopens() {
        let breaker = CircuitBreaker::new(1, Duration::from_secs(60), Duration::from_millis(30));
        let _ = breaker.execute("dep", fail).await;
        assert_eq!(breaker.state(), CircuitState::Open);
        tokio::time::sleep(Duration::from_millis(60)).await;
        let _ = breaker.execute("dep", fail).await; // half-open probe fails
        assert_eq!(breaker.state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn non_retryable_errors_do_not_open_circuit() {
        let breaker = CircuitBreaker::new(1, Duration::from_secs(60), Duration::from_secs(3600));
        for _ in 0..3 {
            let err = breaker
                .execute("dep", || async {
                    Err::<i32, _>(AiError::Validation(ai_errors::ValidationError::new("nope")))
                })
                .await
                .unwrap_err();
            assert!(matches!(err, AiError::Validation(_)));
        }
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn abandoned_probe_restores_permit_and_breaker_recovers() {
        let breaker = std::sync::Arc::new(CircuitBreaker::new(
            2,
            Duration::from_secs(60),
            Duration::from_millis(30),
        ));
        let _ = breaker.execute("dep", fail).await;
        let _ = breaker.execute("dep", fail).await;
        assert_eq!(breaker.state(), CircuitState::Open);

        // Wait past the recovery timeout, then start a probe and abandon it
        // mid-operation — the cancellation/timeout race. Aborting the task
        // drops the `execute` future before any verdict is recorded.
        tokio::time::sleep(Duration::from_millis(80)).await;
        let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn({
            let breaker = std::sync::Arc::clone(&breaker);
            async move {
                let _ = breaker
                    .execute("dep", move || async move {
                        let _ = started_tx.send(());
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        Ok::<i32, AiError>(1)
                    })
                    .await;
            }
        });
        started_rx.await.expect("probe acquired its permit");
        handle.abort();
        let _ = handle.await;

        assert_eq!(breaker.state(), CircuitState::HalfOpen);
        assert_eq!(
            breaker.available_half_open_permits(),
            1,
            "abandoned probe must restore its permit"
        );

        // No permanent fail-fast: a fresh probe must still be admitted.
        let value = breaker
            .execute("dep", succeed)
            .await
            .expect("breaker must admit a probe after an abandoned one");
        assert_eq!(value, 1);
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn repeated_cycles_leak_no_permits() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(60), Duration::from_millis(20));
        for _ in 0..8 {
            let _ = breaker.execute("dep", fail).await;
            let _ = breaker.execute("dep", fail).await;
            assert_eq!(breaker.state(), CircuitState::Open);
            tokio::time::sleep(Duration::from_millis(45)).await;
            breaker
                .execute("dep", succeed)
                .await
                .expect("probe must be admitted on every cycle");
            assert_eq!(breaker.state(), CircuitState::Closed);
            assert_eq!(breaker.available_half_open_permits(), 0);
        }
    }
}
