//! Parallel execution engine for the AI SDK.
//!
//! First-class concurrency control — never blind task spawning:
//!
//! - [`RetryPolicy`] / [`retry`] — error-class-aware retries with
//!   exponential backoff and jitter.
//! - [`ConcurrencyLimiter`] — bounded concurrency keyed by provider/model/
//!   tool (or any logical resource).
//! - [`Parallel`] — fan-out/fan-in with bounded concurrency, a whole-batch
//!   deadline, cancellation, partial results and aggregation.
//! - [`race`] / [`fallback`] — first-success and provider-fallback
//!   strategies.
//! - [`CircuitBreaker`] — failure thresholds, recovery timeout, half-open
//!   probing.
//! - [`ResilientModel`] / [`FallbackModel`] — the same primitives composed
//!   into `ai-core::Model` decorators (retries + breaker + bounded
//!   concurrency + per-attempt timeouts; ordered replica failover), plus
//!   `install_resilience` / `install_fallback_chain` helpers that wire them
//!   onto an [`ai_core::AiClient`] through its model-registration seam.
//! - [`chaos`] — a deterministic fault-injecting HTTP server used by the
//!   test-suite to prove SLOs under injected failures.
//!
//! All operations are cancellation-safe: dropping the future stops the work
//! (timeouts and cancellations propagate through `tokio`).

pub mod chaos;
mod circuit_breaker;
mod concurrency;
mod parallel;
mod resilient;
mod retry;

pub use circuit_breaker::{CircuitBreaker, CircuitState};
pub use concurrency::{ConcurrencyLimiter, Permit};
pub use parallel::{Parallel, ParallelResult, Task, fallback, race};
pub use resilient::{
    FallbackModel, ResiliencePolicy, ResilienceSnapshot, ResilientModel, install_fallback_chain,
    install_resilience,
};
pub use retry::{RetryPolicy, backoff_delay, retry};

/// Convenience alias for the runtime result type.
pub type Result<T, E = ai_errors::AiError> = ai_errors::Result<T, E>;
