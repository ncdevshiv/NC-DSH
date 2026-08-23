//! AEGIS chaos proof — SLO tests.
//!
//! SLO-1: against a chaos server injecting ~31% mixed faults per request,
//! a [`ResilientModel`] with sane retries must deliver **≥ 99% eventual
//! success** over 200 real reqwest-backed calls, and the run must leave a
//! machine-readable proof artifact at `target/aegis-report.json`.
//!
//! SLO-2: a [`FallbackModel`] over an always-failing primary and a healthy
//! alternate must yield 100% success and report the correct served replica.

mod common;

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use common::{HttpModel, ScriptedModel, percentile, ping_request};

use ai_core::Model;
use ai_runtime::RetryPolicy;
use ai_runtime::chaos::{ChaosKnobs, ChaosServer};
use ai_runtime::{FallbackModel, ResiliencePolicy, ResilientModel};

/// The SLO under proof: ≥ 99% of calls must eventually succeed.
const SLO_SUCCESS_RATE: f64 = 0.99;
const TOTAL_CALLS: usize = 200;

fn chaos_knobs() -> ChaosKnobs {
    // ~31% of requests receive some injected fault; every fault class is
    // recoverable by the retry policy under test (drops/stalls/5xx/429 are
    // retryable by classification; garbage bodies map to provider-502).
    ChaosKnobs {
        seed: 0xAE15_0001,
        healthy_latency_ms: AtomicU64::new(20),
        drop_connection_pct: AtomicU64::new(8),
        stall_past_deadline_pct: AtomicU64::new(6),
        stall_ms: AtomicU64::new(1_200),
        http_500_pct: AtomicU64::new(8),
        http_429_pct: AtomicU64::new(4),
        garbage_body_pct: AtomicU64::new(5),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slo1_resilient_model_holds_ninety_nine_percent_under_chaos() {
    let server = ChaosServer::start(chaos_knobs())
        .await
        .expect("chaos server binds");

    let bare = HttpModel::arc(&server.url(), "slo-model");
    let policy = ResiliencePolicy::new()
        .with_retry(
            RetryPolicy::default()
                .with_max_attempts(6)
                .with_base_delay(Duration::from_millis(15))
                .with_max_delay(Duration::from_millis(150))
                .with_jitter(0.3),
        )
        .with_per_attempt_timeout(Duration::from_millis(300));
    let model = Arc::new(ResilientModel::new(bare as Arc<dyn ai_core::Model>, policy));

    let mut latencies_ms: Vec<u128> = Vec::with_capacity(TOTAL_CALLS);
    let mut failures: Vec<String> = Vec::new();
    let run_started = Instant::now();

    // ---- Phase 1: sequential traffic --------------------------------------
    for _ in 0..TOTAL_CALLS / 2 {
        let started = Instant::now();
        match model.generate(ping_request()).await {
            Ok(_) => latencies_ms.push(started.elapsed().as_millis()),
            Err(err) => failures.push(err.to_string()),
        }
    }

    // ---- Phase 2: concurrent traffic (10 workers × 10 calls) --------------
    let mut handles = Vec::new();
    for worker in 0..10 {
        let model = Arc::clone(&model);
        handles.push(tokio::spawn(async move {
            let mut local_latencies = Vec::new();
            let mut local_failures = Vec::new();
            for _ in 0..TOTAL_CALLS / 20 {
                let started = Instant::now();
                match model.generate(ping_request()).await {
                    Ok(_) => local_latencies.push(started.elapsed().as_millis()),
                    Err(err) => local_failures.push(format!("worker {worker}: {err}")),
                }
            }
            (local_latencies, local_failures)
        }));
    }
    for handle in handles {
        let (lat, fails) = handle.await.expect("worker joins");
        latencies_ms.extend(lat);
        failures.extend(fails);
    }

    let elapsed = run_started.elapsed();
    let successes = latencies_ms.len();
    let success_rate = successes as f64 / TOTAL_CALLS as f64;
    let resilience = model.metrics();
    let faults = server.metrics();

    // ---- The assertion that matters ---------------------------------------
    assert!(
        successes >= 198 && success_rate >= SLO_SUCCESS_RATE,
        "SLO-1 violated: {successes}/{TOTAL_CALLS} succeeded ({success_rate:.3}); failures: {:?}",
        failures
    );

    let p50 = percentile(&mut latencies_ms, 50.0);
    let p95 = percentile(&mut latencies_ms, 95.0);
    let p99 = percentile(&mut latencies_ms, 99.0);

    // ---- Proof artifact ----------------------------------------------------
    let report = serde_json::json!({
        "suite": "AEGIS",
        "test": "slo-1-resilience-under-chaos",
        "generated_at_unix_secs": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        "fault_mix_pct": {
            "drop_connection": 8,
            "stall_past_deadline": 6,
            "http_500": 8,
            "http_429": 4,
            "garbage_body": 5,
            "total_configured": 31
        },
        "totals": {
            "calls": TOTAL_CALLS,
            "successful": successes,
            "failed": failures.len(),
            "success_rate": success_rate,
            "wall_clock_ms": elapsed.as_millis() as u64
        },
        "retries": {
            "underlying_attempts": resilience.attempts,
            "retries_performed": resilience.retries,
            "attempt_timeouts": resilience.timeouts
        },
        "server_observed_faults": {
            "requests_served": faults.requests_served,
            "connections_dropped": faults.connections_dropped,
            "stalls": faults.stalls,
            "responses_500": faults.responses_500,
            "responses_429": faults.responses_429,
            "garbage_bodies": faults.garbage_bodies,
            "healthy_responses": faults.healthy_responses
        },
        "latency_ms": {
            "p50": p50,
            "p95": p95,
            "p99": p99
        },
        "slo": {
            "target_success_rate": SLO_SUCCESS_RATE,
            "min_successes": 198,
            "met": successes >= 198
        }
    });

    let report_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/aegis-report.json"
    );
    if let Some(parent) = std::path::Path::new(report_path).parent() {
        std::fs::create_dir_all(parent).expect("target dir is creatable");
    }
    std::fs::write(report_path, serde_json::to_string_pretty(&report).unwrap())
        .expect("report artifact is writable");

    eprintln!(
        "SLO-1: {successes}/{TOTAL_CALLS} ok ({rate:.2}%), p50={p50}ms p95={p95}ms p99={p99}ms; report → {report_path}",
        rate = success_rate * 100.0
    );

    server.shutdown().await;
}

#[tokio::test]
async fn slo2_fallback_chain_yields_full_success_and_correct_replica() {
    let failing_primary = ScriptedModel::always_fail("primary region down");
    let healthy_alternate = ScriptedModel::always_ok("alternate served");

    // Replicas carry their own (small) retry budgets: fallback triggers on
    // non-retryable-after-retries exhaustion inside each replica.
    let replica_policy = ResiliencePolicy::new().with_retry(
        RetryPolicy::default()
            .with_max_attempts(2)
            .with_base_delay(Duration::from_millis(1))
            .with_jitter(0.0),
    );

    let chain = FallbackModel::new(Arc::new(ResilientModel::new(
        failing_primary.clone() as Arc<dyn ai_core::Model>,
        replica_policy.clone(),
    )))
    .with_alternate(Arc::new(ResilientModel::new(
        healthy_alternate.clone() as Arc<dyn ai_core::Model>,
        replica_policy.clone(),
    )));

    const CALLS: usize = 30;
    for call in 0..CALLS {
        let completion = chain.generate(ping_request()).await.unwrap_or_else(|err| {
            panic!("call {call}: fallback chain must absorb total primary failure ({err})")
        });
        assert_eq!(completion.text, "alternate served");
        assert_eq!(
            chain.last_served_index(),
            Some(1),
            "the alternate replica must be reported as serving"
        );
    }
    // Every call exhausted the primary's own retries first (2 attempts × 30).
    assert_eq!(failing_primary.call_count(), (CALLS * 2) as u32);
    assert_eq!(healthy_alternate.call_count(), CALLS as u32);

    // Negative control: when *every* replica fails, the chain surfaces an
    // error instead of pretending success.
    let all_down = FallbackModel::new(Arc::new(ResilientModel::new(
        ScriptedModel::always_fail("replica-a down") as Arc<dyn ai_core::Model>,
        ResiliencePolicy::new().with_retry(RetryPolicy::none()),
    )))
    .with_alternate(Arc::new(ResilientModel::new(
        ScriptedModel::always_fail("replica-b down") as Arc<dyn ai_core::Model>,
        ResiliencePolicy::new().with_retry(RetryPolicy::none()),
    )));
    let err = match all_down.generate(ping_request()).await {
        Ok(_) => panic!("all-fail chain must not succeed"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("replica-b down"), "{err}");
}
