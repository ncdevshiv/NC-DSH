//! HERCULES live benchmark hook (PRD §3.10): runs a 1,000-task fan-out
//! against the real project gateway with bounded concurrency and writes
//! latency/throughput/cost metrics to `target/hercules-report.json`.
//!
//! This test is `#[ignore]`d AND env-gated so normal suites never hit the
//! network (mirroring `crates/ai-providers/tests/live_gateway.rs`):
//!
//! Required env:
//! - `AI_SDK_GATEWAY_API_KEY`  (presence enables the bench)
//! - `AI_SDK_GATEWAY_BASE_URL`
//!
//! Optional env:
//! - `AI_SDK_PRIMARY_MODEL`            (default: `deepseek-v4-flash`)
//! - `HERCULES_BENCH_TASKS`            (default: `1000`)
//! - `HERCULES_BENCH_CONCURRENCY`      (default: `16`)
//! - `HERCULES_BENCH_USD_PER_MTOK_IN`  (default: `0`, input $/1M tokens)
//! - `HERCULES_BENCH_USD_PER_MTOK_OUT` (default: `0`, output $/1M tokens)
//!
//! Run it with:
//!
//! ```text
//! AI_SDK_GATEWAY_BASE_URL=https://opencode.ai/zen/go/v1 \
//! AI_SDK_GATEWAY_API_KEY=... \
//! cargo test -p ai-agents --test hercules_bench -- --ignored --nocapture
//! ```
//!
//! The report lands at `<workspace>/target/hercules-report.json`.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;

use ai_agents::{Agent, AgentBuilder, SwarmEngine};
use ai_core::{ChatRequest, Completion, EventStream, Model};
use ai_errors::AiError;
use ai_models::ModelRegistry;

/// Records the wall-clock duration of every generate() call. Each bench
/// task makes exactly one model call (retries are disabled), so per-call
/// latencies ARE per-task latencies.
struct TimingModel {
    inner: Arc<dyn Model>,
    durations_ms: Arc<Mutex<Vec<u128>>>,
}

#[async_trait]
impl Model for TimingModel {
    fn info(&self) -> &ai_models::ModelInfo {
        self.inner.info()
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
        let started = Instant::now();
        let outcome = self.inner.generate(request).await;
        self.durations_ms
            .lock()
            .expect("timing lock")
            .push(started.elapsed().as_millis());
        outcome
    }

    async fn stream(&self, request: ChatRequest) -> Result<EventStream, AiError> {
        self.inner.stream(request).await
    }
}

fn percentile(sorted: &[u128], p: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "live benchmark: hits the real gateway; run with -- --ignored and gateway env set"]
async fn hercules_bench_1000_task_fan_out() {
    let Some(api_key) = std::env::var("AI_SDK_GATEWAY_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
    else {
        eprintln!("SKIP: AI_SDK_GATEWAY_API_KEY not set (live bench stays ignored)");
        return;
    };
    let base_url = std::env::var("AI_SDK_GATEWAY_BASE_URL")
        .unwrap_or_else(|_| "https://opencode.ai/zen/go/v1".to_string());
    let primary_model =
        std::env::var("AI_SDK_PRIMARY_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into());
    let tasks: usize = std::env::var("HERCULES_BENCH_TASKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let concurrency: usize = std::env::var("HERCULES_BENCH_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16);
    let usd_per_mtok_in: f64 = std::env::var("HERCULES_BENCH_USD_PER_MTOK_IN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let usd_per_mtok_out: f64 = std::env::var("HERCULES_BENCH_USD_PER_MTOK_OUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);

    // Gateway client (same conventions as ai-providers live tests).
    let provider = Arc::new(
        ai_providers::openai_compat::OpenAiCompatProvider::new(
            ai_providers::openai_compat::OpenAiCompatConfig::new(
                "opencode".to_string(),
                api_key,
                base_url,
            ),
        )
        .expect("provider builds"),
    );
    let client = ai_core::AiClient::builder()
        .provider(provider)
        .registry(ModelRegistry::new())
        .build()
        .expect("client builds");
    let reference = format!("opencode:{primary_model}");
    let (_provider_name, resolved) = client.resolve_model(&reference).expect("model resolves");

    // Per-call timing wrapper around the real model.
    let durations_ms = Arc::new(Mutex::new(Vec::new()));
    let timed_model: Arc<dyn Model> = Arc::new(TimingModel {
        inner: resolved,
        durations_ms: durations_ms.clone(),
    });

    // One isolated agent per task via the template (shared model Arc —
    // cheap; fresh memory + own id per task).
    let instructions = "Reply with exactly the number from the user message and nothing else.";
    let template_for_engine: ai_agents::SwarmTemplate = Arc::new(move |index| -> Agent {
        AgentBuilder::new(format!("bench-{index}"), instructions, timed_model.clone())
            // Single-shot benchmarking: failures count in the success rate.
            .with_retry(ai_runtime::RetryPolicy::none())
            .build()
    });

    let inputs: Vec<String> = (0..tasks)
        .map(|i| format!("Return the number {i} verbatim."))
        .collect();

    let engine = SwarmEngine::new(template_for_engine).with_concurrency(concurrency);

    println!("HERCULES bench: {tasks} tasks, concurrency {concurrency}, model {primary_model}");
    let wall_started = Instant::now();
    let outcome = engine
        .fan_out(inputs)
        .await
        .expect("fan-out completes (failures are collected, not fatal)");
    let wall_ms = wall_started.elapsed().as_millis();

    let mut latencies = durations_ms.lock().expect("timing lock").clone();
    latencies.sort_unstable();
    let p50 = percentile(&latencies, 50.0);
    let p95 = percentile(&latencies, 95.0);
    let mean: u128 = if latencies.is_empty() {
        0
    } else {
        latencies.iter().sum::<u128>() / latencies.len() as u128
    };

    let total_input_tokens: u64 = outcome
        .results
        .iter()
        .flatten()
        .map(|r| r.usage.input_tokens)
        .sum();
    let total_output_tokens: u64 = outcome
        .results
        .iter()
        .flatten()
        .map(|r| r.usage.output_tokens)
        .sum();
    let total_tokens: u64 = total_input_tokens + total_output_tokens;
    let estimated_cost_usd = (total_input_tokens as f64 / 1_000_000.0) * usd_per_mtok_in
        + (total_output_tokens as f64 / 1_000_000.0) * usd_per_mtok_out;
    let success_rate = outcome.succeeded as f64 / tasks.max(1) as f64;

    let report = serde_json::json!({
        "benchmark": "hercules-fan-out",
        "model": primary_model,
        "tasks": tasks,
        "concurrency": concurrency,
        "succeeded": outcome.succeeded,
        "failed": outcome.failed.len(),
        "success_rate": success_rate,
        "wall_ms": wall_ms,
        "latency_ms": { "p50": p50, "p95": p95, "mean": mean },
        "tokens": {
            "input": total_input_tokens,
            "output": total_output_tokens,
            "total": total_tokens,
        },
        "estimated_cost_usd": estimated_cost_usd,
        "price_usd_per_mtok": { "input": usd_per_mtok_in, "output": usd_per_mtok_out },
        "generated_at": time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    });

    // <workspace>/target/hercules-report.json (crate manifest lives at
    // crates/ai-agents).
    let workspace_target = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("target"))
        .unwrap_or_else(|| std::path::PathBuf::from("target"));
    std::fs::create_dir_all(&workspace_target).expect("target dir exists");
    let report_path = workspace_target.join("hercules-report.json");
    std::fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).expect("report serializes"),
    )
    .expect("report written");

    println!(
        "HERCULES bench done: {}/{} succeeded ({:.1}%), p50 {} ms, p95 {} ms, \
         {} tokens, ~${:.4}, wall {} ms",
        outcome.succeeded,
        tasks,
        success_rate * 100.0,
        p50,
        p95,
        total_tokens,
        estimated_cost_usd,
        wall_ms
    );
    println!("report: {}", report_path.display());

    // Sanity only when the gateway actually served traffic.
    if outcome.succeeded > 0 {
        assert!(total_tokens > 0, "successful calls must report usage");
    }
}
