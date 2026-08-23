//! Resilience decorators for [`ai_core::Model`] implementations.
//!
//! This module turns the proven runtime primitives — [`RetryPolicy`],
//! [`CircuitBreaker`], and [`ConcurrencyLimiter`] — into transparent
//! decorators around any `Arc<dyn Model>`:
//!
//! - [`ResilientModel`] wraps **one** replica: bounded concurrency →
//!   circuit breaker → retry loop with per-attempt timeout.
//! - [`FallbackModel`] chains several replicas (each typically a
//!   `ResilientModel`) and fails over in order, exposing which replica
//!   served via [`FallbackModel::last_served_index`].
//!
//! # Layering (outermost first)
//!
//! ```text
//! generate(request)
//!   └─ ConcurrencyLimiter permit   (held across the whole call)
//!       └─ CircuitBreaker::execute (fail fast when open; verdict on the
//!           │                       *final* retry outcome, exactly like a
//!           │                       direct `execute()` call would record)
//!           └─ retry loop          (retries only when `AiError::is_retryable()`
//!               │                   or the per-attempt timeout elapsed;
//!               │                   honors server `Retry-After`)
//!               └─ tokio::time::timeout(per_attempt_timeout,
//!                                       inner.generate(request))
//! ```
//!
//! A failure is therefore retried iff `(policy.retryable)(err)` is true **or**
//! the attempt timed out (`AiError::Timeout`); the breaker sees the final
//! outcome of the whole retry loop and records a failure for retryable/timeout
//! errors — mirroring `CircuitBreaker::execute`'s own classification.
//!
//! # Timeout semantics
//!
//! `per_attempt_timeout` bounds **each underlying call**, not the whole retry
//! sequence: one stalled response consumes its own timeout and the remaining
//! attempts still run. Total worst-case latency is thus roughly
//! `max_attempts × (per_attempt_timeout + max_delay)`. For [`Model::stream`]
//! the timeout applies to **establishing** the stream only; body chunks are
//! not individually timed (a slow-but-flowing stream never trips it). The
//! concurrency permit for a streaming call is held until the returned stream
//! is dropped, so long streams keep occupying their budget slot.
//!
//! # Wiring into `AiClient` without a dependency cycle
//!
//! `ai-core` cannot depend on this crate (`ai-runtime` already depends on
//! `ai-core`). The seam is [`ai_core::AiClient::register_model`]: higher-level
//! code resolves bare models through the client, decorates them here, and
//! registers the finished `Arc<dyn Model>` back under the same reference. The
//! helpers below automate that:
//!
//! - [`install_resilience`] — decorate individual model references with a
//!   [`ResiliencePolicy`].
//! - [`install_fallback_chain`] — build a [`FallbackModel`] over an ordered
//!   chain of references and register it under the primary reference.
//!
//! Both are no-ops at the client level when unused: a client built without
//! registrations behaves exactly as before.
//!
//! (An alternative seam — injecting a wrapping closure via something like
//! `with_model_decorator(Arc<dyn Fn(Arc<dyn Model>) -> Arc<dyn Model>>)` —
//! was rejected: pre-wrapped registration is explicit, type-safe, keeps
//! decoration logic out of the client entirely, and needs no lifetime
//! plumbing around the decorator.)

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::StreamExt;
use tracing::{debug, warn};

use ai_core::{AiClient, ChatRequest, Completion, EventStream, Model, ModelInfo, StreamEvent};
use ai_errors::{AiError, TimeoutError};

use crate::Result;
use crate::circuit_breaker::CircuitBreaker;
use crate::concurrency::{ConcurrencyLimiter, Permit};
use crate::retry::{RetryPolicy, backoff_delay};

/// Immutable resilience configuration shared by every call through a
/// [`ResilientModel`]. Build with the fluent setters; unset knobs are off.
///
/// Sharing: pass the same `Arc<CircuitBreaker>` / `Arc<ConcurrencyLimiter>`
/// to several policies to share them across replicas ("per key"). When a
/// policy has no explicit breaker, each decorated reference gets its own
/// default breaker (see [`install_resilience`]).
#[derive(Debug, Clone, Default)]
pub struct ResiliencePolicy {
    /// Retry schedule and classifier (default: 3 attempts, 200 ms base).
    pub retry: RetryPolicy,
    /// Optional shared circuit breaker guarding the dependency.
    pub breaker: Option<Arc<CircuitBreaker>>,
    /// Optional per-attempt deadline applied to each underlying call.
    pub per_attempt_timeout: Option<Duration>,
    /// Optional limiter providing the concurrency budget for `limit_key`.
    pub limiter: Option<Arc<ConcurrencyLimiter>>,
    /// Key under which calls acquire their limiter permit (also used as the
    /// breaker's dependency label).
    pub limit_key: Option<String>,
}

impl ResiliencePolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the retry policy.
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Attaches a (possibly shared) circuit breaker.
    pub fn with_breaker(mut self, breaker: Arc<CircuitBreaker>) -> Self {
        self.breaker = Some(breaker);
        self
    }

    /// Bounds each underlying call by `timeout`. See the module docs for the
    /// exact per-attempt / stream-establishment semantics.
    pub fn with_per_attempt_timeout(mut self, timeout: Duration) -> Self {
        self.per_attempt_timeout = Some(timeout);
        self
    }

    /// Bounds concurrency for `key` to `max_concurrent` using a private
    /// limiter (for sharing a budget across models use
    /// [`ResiliencePolicy::with_limiter`] instead).
    pub fn with_concurrency_limit(mut self, key: impl Into<String>, max_concurrent: usize) -> Self {
        let key: String = key.into();
        let limiter = ConcurrencyLimiter::new();
        limiter.set_limit(&key, max_concurrent);
        self.limiter = Some(Arc::new(limiter));
        self.limit_key = Some(key);
        self
    }

    /// Joins an existing (shared) limiter under `key`.
    pub fn with_limiter(
        mut self,
        limiter: Arc<ConcurrencyLimiter>,
        key: impl Into<String>,
    ) -> Self {
        self.limiter = Some(limiter);
        self.limit_key = Some(key.into());
        self
    }

    /// The breaker label for a wrapped model: the configured key if set,
    /// otherwise the model id.
    fn dependency<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.limit_key.as_deref().unwrap_or(fallback)
    }
}

/// Cumulative counters observed by one [`ResilientModel`]. Cheap atomics;
/// snapshot with [`ResilientModel::metrics`].
#[derive(Debug, Default)]
pub struct ResilienceMetrics {
    calls: AtomicU64,
    attempts: AtomicU64,
    retries: AtomicU64,
    timeouts: AtomicU64,
}

/// Point-in-time copy of [`ResilienceMetrics`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResilienceSnapshot {
    /// Calls that entered the decorator (after acquiring any permit).
    pub calls: u64,
    /// Underlying attempts issued (including the first try of every call).
    pub attempts: u64,
    /// Attempts beyond the first — i.e. retries actually performed.
    pub retries: u64,
    /// Attempts that hit the per-attempt timeout.
    pub timeouts: u64,
}

impl ResilienceMetrics {
    fn snapshot(&self) -> ResilienceSnapshot {
        ResilienceSnapshot {
            calls: self.calls.load(Ordering::Relaxed),
            attempts: self.attempts.load(Ordering::Relaxed),
            retries: self.retries.load(Ordering::Relaxed),
            timeouts: self.timeouts.load(Ordering::Relaxed),
        }
    }
}

/// A [`Model`] decorator adding retries, optional circuit breaking, optional
/// bounded concurrency, and per-attempt timeouts to any replica. See the
/// module docs for the full layering and semantics.
pub struct ResilientModel {
    inner: Arc<dyn Model>,
    policy: ResiliencePolicy,
    metrics: Arc<ResilienceMetrics>,
}

impl std::fmt::Debug for ResilientModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResilientModel")
            .field("inner", &self.inner.info().to_string())
            .field("max_attempts", &self.policy.retry.max_attempts)
            .field("per_attempt_timeout", &self.policy.per_attempt_timeout)
            .field("limit_key", &self.policy.limit_key)
            .finish()
    }
}

impl ResilientModel {
    /// Wraps `inner` with `policy`.
    pub fn new(inner: Arc<dyn Model>, policy: ResiliencePolicy) -> Self {
        Self {
            inner,
            policy,
            metrics: Arc::new(ResilienceMetrics::default()),
        }
    }

    /// The undecorated replica.
    pub fn inner(&self) -> &Arc<dyn Model> {
        &self.inner
    }

    /// Live counters.
    pub fn metrics(&self) -> ResilienceSnapshot {
        self.metrics.snapshot()
    }

    async fn acquire_permit(&self) -> Result<Option<Permit>, AiError> {
        match (&self.policy.limiter, self.policy.limit_key.as_deref()) {
            (Some(limiter), Some(key)) => Ok(Some(limiter.acquire(key).await?)),
            _ => Ok(None),
        }
    }

    /// One underlying attempt, optionally bounded by the per-attempt
    /// deadline. Timeouts surface as [`AiError::Timeout`] (retryable).
    async fn attempt_generate(&self, request: &ChatRequest) -> Result<Completion> {
        let inner = Arc::clone(&self.inner);
        let request = request.clone();
        match self.policy.per_attempt_timeout {
            Some(d) => match tokio::time::timeout(d, inner.generate(request)).await {
                Ok(result) => result,
                Err(_elapsed) => Err(AiError::Timeout(TimeoutError::new("resilient.generate", d))),
            },
            None => inner.generate(request).await,
        }
    }

    /// One stream-*establishment* attempt (see module docs: the body of the
    /// returned stream is not covered by the timeout or the retry loop).
    async fn attempt_stream(&self, request: &ChatRequest) -> Result<EventStream> {
        let inner = Arc::clone(&self.inner);
        let request = request.clone();
        match self.policy.per_attempt_timeout {
            Some(d) => match tokio::time::timeout(d, inner.stream(request)).await {
                Ok(result) => result,
                Err(_elapsed) => Err(AiError::Timeout(TimeoutError::new("resilient.stream", d))),
            },
            None => inner.stream(request).await,
        }
    }

    /// The retry loop shared by `generate` and stream establishment.
    ///
    /// Retries iff the configured classifier says so **or** the attempt timed
    /// out; honors a server-provided `Retry-After` over computed backoff
    /// (same discipline as [`crate::retry`]).
    async fn run_with_retries<T, F, Fut>(
        &self,
        metrics_tag: &'static str,
        mut attempt: F,
    ) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T>> + Send,
    {
        let policy = &self.policy.retry;
        let mut last_error: Option<AiError> = None;

        for attempt_no in 0..policy.max_attempts {
            if attempt_no > 0 {
                let delay = match last_error.as_ref() {
                    Some(AiError::RateLimit(rl)) => rl
                        .retry_after
                        .unwrap_or_else(|| backoff_delay(attempt_no - 1, policy)),
                    _ => backoff_delay(attempt_no - 1, policy),
                };
                debug!(
                    operation = metrics_tag,
                    attempt = attempt_no + 1,
                    delay_ms = delay.as_millis() as u64,
                    "resilient retry after backoff"
                );
                self.metrics.retries.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(delay).await;
            }

            self.metrics.attempts.fetch_add(1, Ordering::Relaxed);
            match attempt().await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    let timed_out = matches!(err, AiError::Timeout(_));
                    if timed_out {
                        self.metrics.timeouts.fetch_add(1, Ordering::Relaxed);
                    }
                    let retryable = (policy.retryable)(&err) || timed_out;
                    if retryable && attempt_no + 1 < policy.max_attempts {
                        warn!(
                            operation = metrics_tag,
                            attempt = attempt_no + 1,
                            error = %err,
                            "transient failure inside resilient model"
                        );
                        last_error = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        unreachable!("the retry loop returns on every iteration");
    }

    /// Applies the breaker (when configured) around `operation`, recording
    /// failures for retryable/timeout final outcomes exactly like
    /// [`CircuitBreaker::execute`] does for a direct call.
    async fn guarded<T, F, Fut>(&self, dependency: &str, operation: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>> + Send,
    {
        match &self.policy.breaker {
            Some(breaker) => breaker.execute(dependency, operation).await,
            None => operation().await,
        }
    }
}

#[async_trait]
impl Model for ResilientModel {
    fn info(&self) -> &ModelInfo {
        self.inner.info()
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion> {
        // Held across retries, backoff, and the whole call.
        let _permit = self.acquire_permit().await?;
        self.metrics.calls.fetch_add(1, Ordering::Relaxed);
        let started = Instant::now();
        let dependency = self.policy.dependency(self.inner.info().id.as_str());

        // The breaker's operation is the *entire* retry loop: it observes the
        // final outcome, so one successful retry records success while a
        // fully-exhausted retryable failure records exactly one fault.
        let this = &*self;
        let request = &request;
        let outcome = this
            .guarded(dependency, move || async move {
                this.run_with_retries("generate", || this.attempt_generate(request))
                    .await
            })
            .await;

        debug!(
            dependency,
            ok = outcome.is_ok(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "resilient generate finished"
        );
        outcome
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream> {
        let permit = self.acquire_permit().await?;
        self.metrics.calls.fetch_add(1, Ordering::Relaxed);
        let dependency = self.policy.dependency(self.inner.info().id.as_str());

        let this = &*self;
        let request = &request;
        let established = this
            .guarded(dependency, move || async move {
                this.run_with_retries("stream", || this.attempt_stream(request))
                    .await
            })
            .await?;

        // The permit (and the limiter budget it represents) now lives inside
        // the stream itself and is released when the consumer drops it.
        Ok(Box::pin(PermittedStream {
            inner: established,
            _permit: permit,
        }))
    }
}

/// An [`EventStream`] carrying the concurrency permit acquired for its
/// establishment; dropping the stream releases the budget slot.
struct PermittedStream {
    inner: EventStream,
    _permit: Option<Permit>,
}

impl futures::Stream for PermittedStream {
    type Item = Result<StreamEvent, AiError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // `Pin<Box<_>>` is Unpin, so `poll_next_unpin` applies to the boxed
        // inner stream.
        self.inner.poll_next_unpin(cx)
    }
}

/// An ordered replica chain: `replicas[0]` is the primary, the rest are
/// alternates. Each replica should already be a [`ResilientModel`] (retries
/// happen *inside* a replica; fallback happens when a replica gives up).
///
/// Failover triggers on **any** final error from a replica ("non-retryable-
/// after-retries"); the first success wins and
/// [`FallbackModel::last_served_index`] reports who served. If every replica
/// fails, the last replica's error is returned.
pub struct FallbackModel {
    replicas: Vec<Arc<dyn Model>>,
    last_served: AtomicIsize,
}

impl std::fmt::Debug for FallbackModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FallbackModel")
            .field(
                "replicas",
                &self
                    .replicas
                    .iter()
                    .map(|m| m.info().to_string())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl FallbackModel {
    /// Starts a chain whose primary is `primary`.
    pub fn new(primary: Arc<dyn Model>) -> Self {
        Self {
            replicas: vec![primary],
            last_served: AtomicIsize::new(-1),
        }
    }

    /// Builder-style append of an alternate replica.
    pub fn with_alternate(mut self, alternate: Arc<dyn Model>) -> Self {
        self.replicas.push(alternate);
        self
    }

    /// Number of replicas in the chain (≥ 1).
    pub fn len(&self) -> usize {
        self.replicas.len()
    }

    /// Always `false`: a chain always holds at least its primary. Present so
    /// `len()`-based clippy checks stay satisfied.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Index of the replica that served the most recent successful call
    /// (`None` before the first success).
    pub fn last_served_index(&self) -> Option<usize> {
        let idx = self.last_served.load(Ordering::Relaxed);
        (idx >= 0).then_some(idx as usize)
    }

    fn note_served(&self, index: usize) {
        self.last_served.store(index as isize, Ordering::Relaxed);
    }
}

#[async_trait]
impl Model for FallbackModel {
    fn info(&self) -> &ModelInfo {
        // Chains share the primary's identity; alternates are invisible to
        // routing metadata by design (they are capacity, not catalog).
        self.replicas[0].info()
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion> {
        let mut last_error: Option<AiError> = None;
        for (index, replica) in self.replicas.iter().enumerate() {
            match replica.generate(request.clone()).await {
                Ok(completion) => {
                    self.note_served(index);
                    return Ok(completion);
                }
                Err(err) => {
                    warn!(
                        replica = index,
                        model = %replica.info(),
                        error = %err,
                        "fallback replica failed; trying next"
                    );
                    last_error = Some(err);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            AiError::Internal(ai_errors::InternalError::new(
                "fallback chain had no replicas",
            ))
        }))
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream> {
        let mut last_error: Option<AiError> = None;
        for (index, replica) in self.replicas.iter().enumerate() {
            match replica.stream(request.clone()).await {
                Ok(stream) => {
                    self.note_served(index);
                    return Ok(stream);
                }
                Err(err) => {
                    warn!(
                        replica = index,
                        model = %replica.info(),
                        error = %err,
                        "fallback replica failed to establish stream; trying next"
                    );
                    last_error = Some(err);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            AiError::Internal(ai_errors::InternalError::new(
                "fallback chain had no replicas",
            ))
        }))
    }
}

// ---------------------------------------------------------------------------
// Client wiring helpers (the ai-runtime side of the register_model seam).
// ---------------------------------------------------------------------------

/// Decorates each listed model reference with a [`ResilientModel`] and
/// registers the wrapper back onto `client` under the same reference
/// (overriding provider resolution for those references from then on).
///
/// When `policy` carries no breaker, every reference gets its **own**
/// default breaker; pass [`ResiliencePolicy::with_breaker`] with a shared
/// `Arc<CircuitBreaker>` to trip them together.
///
/// This is the ai-runtime counterpart of a hypothetical
/// `AiClientBuilder::with_resilience(policy)` — see the module docs for why
/// it lives here instead of `ai-core`.
pub fn install_resilience(
    client: &AiClient,
    policy: &ResiliencePolicy,
    references: &[&str],
) -> Result<(), AiError> {
    for reference in references {
        let (_, bare) = client.resolve_model(reference)?;
        client.register_model(
            *reference,
            Arc::new(ResilientModel::new(bare, policy.clone())),
        );
    }
    Ok(())
}

/// Builds a [`FallbackModel`] over an ordered chain of model references —
/// `chain[0]` is the primary, the rest are failover alternates — where every
/// replica is individually wrapped in a [`ResilientModel`] using `policy`,
/// and registers the chain under the primary reference.
///
/// This is the ai-runtime counterpart of a hypothetical
/// `AiClientBuilder::with_fallback_chain(references)`.
pub fn install_fallback_chain(
    client: &AiClient,
    policy: &ResiliencePolicy,
    chain: &[&str],
) -> Result<(), AiError> {
    assert!(
        !chain.is_empty(),
        "fallback chain must contain at least the primary reference"
    );
    let mut replicas: Vec<Arc<dyn Model>> = Vec::with_capacity(chain.len());
    for reference in chain {
        let (_, bare) = client.resolve_model(reference)?;
        replicas.push(Arc::new(ResilientModel::new(bare, policy.clone())));
    }
    let primary = replicas.remove(0);
    let mut fallback = FallbackModel::new(primary);
    for replica in replicas {
        fallback = fallback.with_alternate(replica);
    }
    client.register_model(chain[0], Arc::new(fallback));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicU32;

    use futures::StreamExt;
    use parking_lot::Mutex;

    use ai_core::StreamEvent;
    use ai_errors::{
        InternalError, NetworkError, RateLimitError, SerializationError, ValidationError,
    };
    use ai_types::{ModelId, ProviderId};

    fn model_info(id: &str) -> ModelInfo {
        ModelInfo::new(ProviderId::new("chaos"), ModelId::new(id), 128_000, 8_192)
    }

    fn completion(model: &ScriptedModel, text: &str) -> Completion {
        Completion {
            provider: model.info.provider.clone(),
            model: model.info.id.clone(),
            text: text.to_string(),
            tool_calls: Vec::new(),
            usage: Default::default(),
            reasoning: None,
            raw: serde_json::Value::Null,
            finish_reason: Some("stop".into()),
        }
    }

    /// A model whose `generate` outcomes follow a fixed script; once the
    /// script runs dry it repeats its fallback outcome (a canned OK text or
    /// a freshly-built network error), so tests never hang.
    struct ScriptedModel {
        info: ModelInfo,
        script: Mutex<VecDeque<Result<String>>>,
        fallback_ok: Option<String>,
        fallback_err_msg: Option<&'static str>,
        calls: AtomicU32,
        /// Extra latency applied to the *first* call only (timeout tests).
        first_call_latency: Mutex<Option<Duration>>,
        stream_latency: Mutex<Duration>,
        stream_fails: AtomicU32,
    }

    impl ScriptedModel {
        fn new(
            script: Vec<Result<&str>>,
            ok: Option<&str>,
            err: Option<&'static str>,
        ) -> Arc<Self> {
            Arc::new(Self {
                info: model_info("scripted"),
                script: Mutex::new(script.into_iter().map(|r| r.map(str::to_string)).collect()),
                fallback_ok: ok.map(str::to_string),
                fallback_err_msg: err,
                calls: AtomicU32::new(0),
                first_call_latency: Mutex::new(None),
                stream_latency: Mutex::new(Duration::ZERO),
                stream_fails: AtomicU32::new(0),
            })
        }

        /// A finite script; past its end this model yields an internal error
        /// (tests should never rely on that).
        fn scripted(outcomes: Vec<Result<&str>>) -> Arc<Self> {
            Self::new(outcomes, None, None)
        }

        /// Answers every generate with the same text.
        fn always_ok(text: &'static str) -> Arc<Self> {
            Self::new(Vec::new(), Some(text), None)
        }

        /// Fails every generate with a retryable network error.
        fn always_err(msg: &'static str) -> Arc<Self> {
            Self::new(Vec::new(), None, Some(msg))
        }

        fn with_first_call_latency(self: &Arc<Self>, latency: Duration) -> Arc<Self> {
            *self.first_call_latency.lock() = Some(latency);
            Arc::clone(self)
        }

        fn with_failing_streams(self: &Arc<Self>) -> Arc<Self> {
            self.stream_fails.store(1, Ordering::SeqCst);
            Arc::clone(self)
        }

        fn with_stream_latency(self: &Arc<Self>, latency: Duration) -> Arc<Self> {
            *self.stream_latency.lock() = latency;
            Arc::clone(self)
        }

        fn call_count(&self) -> u32 {
            self.calls.load(Ordering::SeqCst)
        }

        async fn next_outcome(&self) -> Result<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // Read-then-clear so no parking_lot guard is held across `.await`
            // (guards are !Send and would poison the future's Send bound).
            let extra = *self.first_call_latency.lock();
            if let Some(delay) = extra {
                *self.first_call_latency.lock() = None;
                tokio::time::sleep(delay).await;
            }
            match self.script.lock().pop_front() {
                Some(outcome) => outcome,
                None => match (&self.fallback_ok, self.fallback_err_msg) {
                    (Some(text), _) => Ok(text.clone()),
                    (None, Some(msg)) => Err(AiError::Network(NetworkError::new("test", msg))),
                    (None, None) => Err(AiError::Internal(InternalError::new("script exhausted"))),
                },
            }
        }
    }

    #[async_trait]
    impl Model for ScriptedModel {
        fn info(&self) -> &ModelInfo {
            &self.info
        }

        async fn generate(&self, _request: ChatRequest) -> Result<Completion> {
            let text = self.next_outcome().await?;
            Ok(completion(self, &text))
        }

        async fn stream(&self, _request: ChatRequest) -> Result<EventStream> {
            let latency = *self.stream_latency.lock();
            tokio::time::sleep(latency).await;
            if self.stream_fails.load(Ordering::SeqCst) == 1 {
                return Err(AiError::Network(NetworkError::new("stream", "refused")));
            }
            let text = self
                .fallback_ok
                .clone()
                .unwrap_or_else(|| "chunk".to_string());
            Ok(Box::pin(futures::stream::iter(vec![
                Ok(StreamEvent::TextDelta { delta: text }),
                Ok(StreamEvent::Completed {
                    finish_reason: Some("stop".into()),
                }),
            ])))
        }
    }

    fn net_err(msg: &'static str) -> Result<&'static str> {
        Err(AiError::Network(NetworkError::new("test", msg)))
    }

    fn fast_policy(max_attempts: u32) -> ResiliencePolicy {
        ResiliencePolicy::new().with_retry(
            RetryPolicy::default()
                .with_max_attempts(max_attempts)
                .with_base_delay(Duration::from_millis(1))
                .with_jitter(0.0),
        )
    }

    fn request() -> ChatRequest {
        ChatRequest::new(vec![ai_core::Message::text(ai_core::Role::User, "hi")])
    }

    #[tokio::test]
    async fn retries_only_when_error_is_retryable() {
        let flaky = ScriptedModel::scripted(vec![
            net_err("transient"),
            net_err("still transient"),
            Ok("recovered"),
        ]);
        let model = ResilientModel::new(flaky.clone(), fast_policy(4));

        let text = model.generate(request()).await.unwrap().text;
        assert_eq!(text, "recovered");
        assert_eq!(flaky.call_count(), 3);
        let m = model.metrics();
        assert_eq!((m.calls, m.attempts, m.retries), (1, 3, 2));

        // Non-retryable: validation errors surface immediately, no retries.
        let invalid = ScriptedModel::scripted(vec![Err(AiError::Validation(
            ValidationError::new("bad input"),
        ))]);
        let strict = ResilientModel::new(invalid.clone(), fast_policy(5));
        let err = strict.generate(request()).await.unwrap_err();
        assert!(matches!(err, AiError::Validation(_)));
        assert_eq!(invalid.call_count(), 1, "must not retry validation errors");
        assert_eq!(strict.metrics().attempts, 1);
    }

    #[tokio::test]
    async fn timeout_is_retried_and_counted() {
        let slow_first = ScriptedModel::always_ok("late-then-fine")
            .with_first_call_latency(Duration::from_millis(120));
        let policy = fast_policy(3).with_per_attempt_timeout(Duration::from_millis(25));
        let model = ResilientModel::new(slow_first.clone(), policy);

        let text = model.generate(request()).await.unwrap().text;
        assert_eq!(text, "late-then-fine");
        assert_eq!(slow_first.call_count(), 2, "timed-out attempt was retried");
        let m = model.metrics();
        assert_eq!(m.timeouts, 1);
        assert_eq!(m.retries, 1);
    }

    #[tokio::test]
    async fn exhausted_timeouts_return_timeout_error() {
        // Every generate sleeps far past the per-attempt deadline, so both
        // attempts time out and the decorator surfaces `AiError::Timeout`.
        let gated = Arc::new(SlowGenerate {
            info: model_info("slow"),
            latency: Duration::from_millis(60),
        });
        let policy = fast_policy(2).with_per_attempt_timeout(Duration::from_millis(10));
        let model = ResilientModel::new(gated, policy);
        let err = model.generate(request()).await.unwrap_err();
        assert!(matches!(err, AiError::Timeout(_)), "{err}");
        assert_eq!(model.metrics().timeouts, 2);
    }

    struct SlowGenerate {
        info: ModelInfo,
        latency: Duration,
    }

    #[async_trait]
    impl Model for SlowGenerate {
        fn info(&self) -> &ModelInfo {
            &self.info
        }
        async fn generate(&self, _request: ChatRequest) -> Result<Completion> {
            tokio::time::sleep(self.latency).await;
            unreachable!("the per-attempt timeout must cancel this future");
        }
        async fn stream(&self, _request: ChatRequest) -> Result<EventStream> {
            Err(AiError::Internal(InternalError::new("unused")))
        }
    }

    #[tokio::test]
    async fn breaker_opens_on_exhausted_retries_and_fails_fast() {
        let breaker = Arc::new(CircuitBreaker::new(
            2,
            Duration::from_secs(60),
            Duration::from_secs(3600),
        ));
        let down = ScriptedModel::always_err("down");
        // max_attempts = 1 so each decorated call is exactly one breaker verdict.
        let policy = ResiliencePolicy::new()
            .with_retry(RetryPolicy::none())
            .with_breaker(Arc::clone(&breaker));
        let model = ResilientModel::new(down.clone(), policy);

        for _ in 0..2 {
            assert!(model.generate(request()).await.is_err());
        }
        assert_eq!(breaker.state(), crate::CircuitState::Open);
        assert_eq!(down.call_count(), 2);

        // Open circuit fails fast without touching the replica.
        let started = Instant::now();
        let err = model.generate(request()).await.unwrap_err();
        assert!(err.to_string().contains("circuit"), "{err}");
        assert_eq!(down.call_count(), 2, "fail-fast must not reach the replica");
        assert!(started.elapsed() < Duration::from_millis(200));
    }

    #[tokio::test]
    async fn rate_limit_retry_after_is_honored_by_decorator() {
        let limited = ScriptedModel::scripted(vec![
            Err(AiError::RateLimit(
                RateLimitError::new("chaos", "429").with_retry_after(Duration::from_millis(5)),
            )),
            Ok("after-limit"),
        ]);
        let policy = RetryPolicy::default()
            .with_max_attempts(2)
            .with_base_delay(Duration::from_secs(30)) // must NOT be used
            .with_jitter(0.0);
        let model = ResilientModel::new(limited, ResiliencePolicy::new().with_retry(policy));
        let started = Instant::now();
        let text = model.generate(request()).await.unwrap().text;
        let elapsed = started.elapsed();
        assert_eq!(text, "after-limit");
        assert!(
            elapsed < Duration::from_secs(2),
            "Retry-After (5 ms) must win over the 30 s backoff; took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn garbage_body_classification_is_respected() {
        // Serialization errors are NOT retryable by default: the decorator
        // surfaces them immediately (the SLO test classifies malformed
        // payloads as provider-502 instead).
        let garbage = ScriptedModel::scripted(vec![Err(AiError::Serialization(
            SerializationError::new("not json"),
        ))]);
        let model = ResilientModel::new(garbage, fast_policy(4));
        let err = model.generate(request()).await.unwrap_err();
        assert!(matches!(err, AiError::Serialization(_)));
        assert_eq!(model.metrics().attempts, 1);
    }

    #[tokio::test]
    async fn fallback_serves_alternate_and_tracks_index() {
        let down = ScriptedModel::always_err("primary down");
        let up = ScriptedModel::always_ok("from-alternate");
        let policy = ResiliencePolicy::new().with_retry(RetryPolicy::none());

        let chain = FallbackModel::new(Arc::new(ResilientModel::new(down.clone(), policy.clone())))
            .with_alternate(Arc::new(ResilientModel::new(up.clone(), policy)));

        assert_eq!(chain.last_served_index(), None);
        let completion = chain.generate(request()).await.unwrap();
        assert_eq!(completion.text, "from-alternate");
        assert_eq!(chain.last_served_index(), Some(1));
        assert_eq!(down.call_count(), 1);
        assert_eq!(up.call_count(), 1);

        // Primary healthy again → served index flips back to 0.
        let healthy_primary = ScriptedModel::always_ok("primary fine");
        let chain = FallbackModel::new(Arc::new(ResilientModel::new(
            healthy_primary.clone(),
            ResiliencePolicy::new().with_retry(RetryPolicy::none()),
        )))
        .with_alternate(Arc::new(ResilientModel::new(
            ScriptedModel::always_ok("alt"),
            ResiliencePolicy::new(),
        )));
        let completion = chain.generate(request()).await.unwrap();
        assert_eq!(completion.text, "primary fine");
        assert_eq!(chain.last_served_index(), Some(0));
        assert_eq!(healthy_primary.call_count(), 1);
    }

    #[tokio::test]
    async fn fallback_returns_last_error_when_every_replica_fails() {
        let a = ScriptedModel::always_err("replica-a down");
        let b = ScriptedModel::always_err("replica-b down");
        let policy = ResiliencePolicy::new().with_retry(RetryPolicy::none());
        let chain = FallbackModel::new(Arc::new(ResilientModel::new(a, policy.clone())))
            .with_alternate(Arc::new(ResilientModel::new(b, policy)));

        let err = chain.generate(request()).await.unwrap_err();
        assert!(err.to_string().contains("replica-b down"), "{err}");
        assert_eq!(chain.last_served_index(), None);
    }

    #[tokio::test]
    async fn stream_establishment_times_out_but_body_is_not_timed() {
        // Establishment slower than the deadline → Timeout.
        let slow_gate =
            ScriptedModel::always_ok("x").with_stream_latency(Duration::from_millis(120));
        let policy = fast_policy(1).with_per_attempt_timeout(Duration::from_millis(25));
        let model = ResilientModel::new(slow_gate, policy);
        let err = match model.stream(request()).await {
            Err(err) => err,
            Ok(_) => panic!("expected the stream establishment to time out"),
        };
        assert!(matches!(err, AiError::Timeout(_)), "{err}");

        // Fast establishment: events flow afterwards even though the same
        // (short) deadline stays armed for the whole body — proving chunks
        // are not individually timed.
        let fast = ScriptedModel::always_ok("body").with_stream_latency(Duration::from_millis(0));
        let model = ResilientModel::new(fast, fast_policy(1));
        let mut stream = model.stream(request()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await; // longer than the deadline
        let events: Vec<_> = StreamExt::collect(&mut stream).await;
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(events.iter().all(|e| e.is_ok()));
    }

    #[tokio::test]
    async fn stream_falls_over_to_next_replica() {
        let broken = ScriptedModel::always_ok("unused").with_failing_streams();
        let working = ScriptedModel::always_ok("ok-body");
        let policy = ResiliencePolicy::new().with_retry(RetryPolicy::none());
        let chain = FallbackModel::new(Arc::new(ResilientModel::new(broken, policy.clone())))
            .with_alternate(Arc::new(ResilientModel::new(working, policy)));

        let mut stream = chain.stream(request()).await.unwrap();
        let events: Vec<_> = StreamExt::collect(&mut stream).await;
        assert_eq!(events.len(), 2);
        assert_eq!(chain.last_served_index(), Some(1));
    }

    #[tokio::test]
    async fn install_helpers_decorate_client_references_end_to_end() {
        use std::collections::HashMap;

        struct MapProvider {
            models: HashMap<String, Arc<dyn Model>>,
        }
        #[async_trait]
        impl ai_core::Provider for MapProvider {
            fn id(&self) -> &str {
                "mock"
            }
            async fn list_models(&self) -> Result<Vec<ModelInfo>> {
                Ok(self.models.values().map(|m| m.info().clone()).collect())
            }
            fn model(&self, model_id: &str) -> Result<Arc<dyn Model>> {
                self.models.get(model_id).cloned().ok_or_else(|| {
                    AiError::Internal(InternalError::new(format!("no model {model_id}")))
                })
            }
        }

        let a_down = ScriptedModel::always_err("a down");
        let b_down = ScriptedModel::always_err("b down");
        let c_steady = ScriptedModel::always_ok("c steady");

        let mut models: HashMap<String, Arc<dyn Model>> = HashMap::new();
        models.insert("a".into(), a_down.clone() as Arc<dyn Model>);
        models.insert("b".into(), b_down.clone() as Arc<dyn Model>);
        models.insert("c".into(), c_steady.clone() as Arc<dyn Model>);

        let client = ai_core::AiClient::builder()
            .provider(Arc::new(MapProvider { models }))
            .build()
            .unwrap();

        // Single-reference resilience installation.
        install_resilience(
            &client,
            &ResiliencePolicy::new().with_retry(RetryPolicy::none()),
            &["mock:a"],
        )
        .unwrap();

        // Fallback chain over b → c, registered under the primary reference.
        install_fallback_chain(
            &client,
            &ResiliencePolicy::new().with_retry(RetryPolicy::none()),
            &["mock:b", "mock:c"],
        )
        .unwrap();

        // mock:a now resolves to its decorator; it still fails (retry-none),
        // but the call went through the resilience layer.
        let err = client
            .generate(
                "mock:a",
                vec![ai_core::Message::text(ai_core::Role::User, "x")],
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("a down"));

        // The chain reference resolves to the FallbackModel: primary b fails,
        // alternate c wins; the served index is visible on the decorator.
        let completion = client
            .generate(
                "mock:b",
                vec![ai_core::Message::text(ai_core::Role::User, "x")],
            )
            .await
            .unwrap();
        assert_eq!(completion.text, "c steady");
        assert_eq!(client.registered_references(), vec!["mock:a", "mock:b"]);
    }
}
