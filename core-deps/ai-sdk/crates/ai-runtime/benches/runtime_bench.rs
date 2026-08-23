//! Criterion benchmarks for `ai-runtime` primitives.
//!
//! - `parallel_execute/limit_{1,8,64}` — [`Parallel::execute`] fan-out
//!   throughput with 256 trivial (immediately-ready) tasks at each global
//!   concurrency limit. This measures executor overhead (limiter permits,
//!   FuturesUnordered drain, result store), not task payloads.
//! - `retry_backoff_delay/{deterministic,jittered}` — cost of
//!   [`backoff_delay`] computation (exponential backoff + cap + jitter),
//!   a pure-sync hot path invoked before every retry sleep.
//! - `circuit_breaker/closed_cycle` — full [`CircuitBreaker::execute`]
//!   acquire → operation → record-success cycle while Closed: the atomic
//!   state read plus the failure-window mutex traffic of the healthy path.
//!
//! All benches are smoke-safe: CI runs the binaries in criterion's `--test`
//! mode (each benchmark once, no timing assertions).

use std::future::ready;
use std::sync::Arc;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use ai_errors::AiError;
use ai_runtime::{CircuitBreaker, Parallel, RetryPolicy, Task, backoff_delay};

const TASKS_PER_BATCH: usize = 256;

/// Builds one batch of trivial ready-OK tasks (fresh per iteration; a
/// `Task` is single-use).
fn trivial_tasks(n: usize) -> Vec<Task<u64>> {
    (0..n)
        .map(|i| Task::new(format!("task-{i}"), ready(Ok(i as u64))))
        .collect()
}

fn bench_parallel_execute(c: &mut Criterion) {
    let rt = Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime"));

    let mut group = c.benchmark_group("parallel_execute");
    // Throughput = completed tasks per batch.
    group.throughput(Throughput::Elements(TASKS_PER_BATCH as u64));
    for limit in [1usize, 8, 64] {
        group.bench_function(BenchmarkId::new("limit", limit), |b| {
            let rt = Arc::clone(&rt);
            b.iter_batched(
                || trivial_tasks(TASKS_PER_BATCH),
                |tasks| {
                    let parallel = Parallel::new().with_limit(limit);
                    let results = rt.block_on(parallel.execute(tasks));
                    assert!(results.iter().all(|r| r.succeeded()));
                    results.len()
                },
                criterion::BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

fn bench_retry_backoff_delay(c: &mut Criterion) {
    let deterministic = RetryPolicy::default()
        .with_max_attempts(8)
        .with_base_delay(Duration::from_millis(200))
        .with_jitter(0.0);
    let jittered = RetryPolicy::default()
        .with_max_attempts(8)
        .with_base_delay(Duration::from_millis(200))
        .with_jitter(0.2);

    let mut group = c.benchmark_group("retry_backoff_delay");
    group.bench_function("deterministic", |b| {
        b.iter(|| {
            // Sweep attempts 0..=7 to mirror a worst-case retry ladder.
            for attempt in 0..8u32 {
                let d = backoff_delay(attempt, &deterministic);
                assert!(d > Duration::ZERO || attempt == 0);
            }
        })
    });
    group.bench_function("jittered", |b| {
        b.iter(|| {
            for attempt in 0..8u32 {
                // The cap is applied before jitter, so a jittered delay may
                // exceed max_delay by up to the jitter fraction — only
                // sanity-check the floor here.
                let d = backoff_delay(attempt, &jittered);
                assert!(d > Duration::ZERO);
            }
        })
    });
    group.finish();
}

fn bench_circuit_breaker_closed(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    // One breaker reused across iterations: every cycle succeeds and stays
    // Closed, so this measures exactly the healthy-path cost (atomic state
    // read in try_acquire + failure-window clear under the mutex in
    // record_success).
    let breaker = Arc::new(CircuitBreaker::defaults());
    assert_eq!(breaker.state(), ai_runtime::CircuitState::Closed);

    let mut group = c.benchmark_group("circuit_breaker");
    group.bench_function("closed_acquire_record_cycle", |b| {
        b.to_async(&rt).iter(|| {
            let breaker = Arc::clone(&breaker);
            async move {
                let value: u64 = breaker
                    .execute("dependency", || ready(Ok::<u64, AiError>(42)))
                    .await
                    .expect("closed circuit must admit calls");
                assert_eq!(value, 42);
            }
        })
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_millis(500));
    targets = bench_parallel_execute, bench_retry_backoff_delay, bench_circuit_breaker_closed
}
criterion_main!(benches);
