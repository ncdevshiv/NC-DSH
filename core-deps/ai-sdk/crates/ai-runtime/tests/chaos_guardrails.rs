//! AEGIS chaos proof — guardrail tests.
//!
//! - The circuit breaker must **open** under sustained chaos (fail fast
//!   without touching the dependency), **half-open** after the recovery
//!   timeout, close on a successful probe, and re-open on a failed probe.
//! - The concurrency limiter must bound in-flight calls as observed from the
//!   *server's* side, not from client bookkeeping.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use common::{HttpModel, ping_request};

use ai_core::Model;
use ai_runtime::chaos::{ChaosKnobs, ChaosServer};
use ai_runtime::{
    CircuitBreaker, CircuitState, ConcurrencyLimiter, ResiliencePolicy, ResilientModel,
};

fn breaker_knobs(http_500_pct: u64) -> ChaosKnobs {
    ChaosKnobs {
        seed: 0xBEA12E01,
        healthy_latency_ms: AtomicU64::new(5),
        drop_connection_pct: AtomicU64::new(0),
        stall_past_deadline_pct: AtomicU64::new(0),
        stall_ms: AtomicU64::new(0),
        http_500_pct: AtomicU64::new(http_500_pct),
        http_429_pct: AtomicU64::new(0),
        garbage_body_pct: AtomicU64::new(0),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn breaker_opens_under_sustained_chaos_then_half_opens_and_recovers() {
    // 95% of requests fail with provider-500 → the breaker must trip fast.
    let server = ChaosServer::start(breaker_knobs(95))
        .await
        .expect("chaos server binds");

    let bare = HttpModel::arc(&server.url(), "breaker-model");
    let breaker = Arc::new(CircuitBreaker::new(
        4,
        Duration::from_secs(60),
        Duration::from_millis(150), // short recovery window so the test stays quick
    ));
    let policy = ResiliencePolicy::new()
        .with_retry(ai_runtime::RetryPolicy::none())
        .with_breaker(Arc::clone(&breaker));
    let model = Arc::new(ResilientModel::new(bare as Arc<dyn Model>, policy));

    // --- Sustained chaos drives Closed → Open -------------------------------
    let mut saw_open = false;
    for _ in 0..12 {
        let _ = model.generate(ping_request()).await;
        if breaker.state() == CircuitState::Open {
            saw_open = true;
            break;
        }
    }
    assert!(
        saw_open,
        "breaker never opened after 12 failing calls; state={:?}",
        breaker.state()
    );

    // --- While open, calls fail fast without reaching the server ------------
    let served_before = server.metrics().requests_served;
    for _ in 0..3 {
        let started = std::time::Instant::now();
        let err = model.generate(ping_request()).await.unwrap_err();
        assert!(err.to_string().contains("circuit"), "{err}");
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "open-circuit call took {:?}; expected fail-fast",
            started.elapsed()
        );
    }
    let served_after = server.metrics().requests_served;
    assert_eq!(
        served_before, served_after,
        "open circuit leaked requests to the dependency"
    );

    // --- Heal the dependency; the recovery window elapses → HalfOpen probe --
    server.knobs().http_500_pct.store(0, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(220)).await; // > recovery_timeout

    let completion = model
        .generate(ping_request())
        .await
        .expect("probe after recovery must pass once the dependency healed");
    assert!(completion.text.starts_with("pong"));
    assert_eq!(
        breaker.state(),
        CircuitState::Closed,
        "successful half-open probe must close the circuit"
    );

    // --- A failed *half-open probe* re-opens ---------------------------------
    // Re-trip the breaker under full chaos (Closed needs the full threshold).
    server.knobs().http_500_pct.store(100, Ordering::SeqCst);
    for _ in 0..4 {
        let _ = model.generate(ping_request()).await;
    }
    assert_eq!(breaker.state(), CircuitState::Open);

    // Recovery window elapses: the next call becomes the half-open probe.
    // It hits a still-failing dependency and must re-open immediately.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = model.generate(ping_request()).await.unwrap_err();
    assert_eq!(
        breaker.state(),
        CircuitState::Open,
        "a failed half-open probe must re-open the circuit"
    );

    // --- And healing again lets a fresh probe close it -----------------------
    server.knobs().http_500_pct.store(0, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(200)).await;
    model
        .generate(ping_request())
        .await
        .expect("healed dependency admits a fresh probe");
    assert_eq!(breaker.state(), CircuitState::Closed);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn limiter_bounds_server_observed_in_flight_calls() {
    // Healthy server with real latency so concurrency is observable.
    let knobs = ChaosKnobs {
        healthy_latency_ms: AtomicU64::new(60),
        ..ChaosKnobs::healthy(0xC0DE_C001)
    };
    let server = ChaosServer::start(knobs).await.expect("chaos server binds");

    let bare = HttpModel::arc(&server.url(), "limited-model");
    let limiter = Arc::new(ConcurrencyLimiter::new());
    const LIMIT: usize = 3;
    limiter.set_limit("chaos:model", LIMIT);

    let policy = ResiliencePolicy::new()
        .with_retry(ai_runtime::RetryPolicy::none())
        .with_limiter(Arc::clone(&limiter), "chaos:model");
    let model = Arc::new(ResilientModel::new(bare as Arc<dyn Model>, policy));

    const CALLS: usize = 12;
    let mut handles = Vec::new();
    for _ in 0..CALLS {
        let model = Arc::clone(&model);
        handles.push(tokio::spawn(
            async move { model.generate(ping_request()).await },
        ));
    }
    let mut ok = 0;
    for handle in handles {
        handle.await.expect("task joins").expect("healthy call");
        ok += 1;
    }
    assert_eq!(ok, CALLS);

    let metrics = server.metrics();
    assert!(
        metrics.max_in_flight <= LIMIT as u64,
        "server observed {} concurrent requests; limit is {LIMIT}",
        metrics.max_in_flight
    );
    assert!(
        metrics.max_in_flight >= 2,
        "sanity: traffic actually overlapped on the server"
    );

    eprintln!(
        "limiter guardrail: {ok} calls, max in-flight observed by server = {} (limit {LIMIT})",
        metrics.max_in_flight
    );

    server.shutdown().await;
}
