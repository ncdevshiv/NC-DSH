//! Live end-to-end proof: a real prompt flows through the full orchestrator
//! pipeline — ambiguity assessment, LLM decomposition into a task tree,
//! pooled derived agents executing each leaf — against the real gateway.
//!
//! Credential-gated and `#[ignore]`d (same conventions as
//! `ai-providers/tests/live_gateway.rs`); run with:
//!
//! ```bash
//! cargo test -p ai-orchestra --test orchestra_live -- --ignored --nocapture
//! ```
//!
//! Required env (see `.env.example`): `AI_SDK_GATEWAY_BASE_URL`,
//! `AI_SDK_GATEWAY_API_KEY`, `AI_SDK_PRIMARY_MODEL`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ai_agents::AgentBuilder;
use ai_core::Provider;
use ai_orchestra::clarifier::LlmPlanner;
use ai_orchestra::orchestra::{Orchestrator, OrchestratorConfig};
use ai_orchestra::watchdog::WatchdogConfig;
use ai_orchestra::{AgentRegistry, NodeStatus, Planner, QuestionMailbox};

struct Gateway {
    provider_id: String,
    base_url: String,
    api_key: String,
    primary_model: String,
}

fn gateway_from_env() -> Option<Gateway> {
    let base_url = std::env::var("AI_SDK_GATEWAY_BASE_URL").ok()?;
    let api_key = std::env::var("AI_SDK_GATEWAY_API_KEY").ok()?;
    if api_key.trim().is_empty() || api_key.contains("your-key-here") {
        return None;
    }
    let primary_model =
        std::env::var("AI_SDK_PRIMARY_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".to_string());
    Some(Gateway {
        provider_id: "opencode".to_string(),
        base_url,
        api_key,
        primary_model,
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires AI_SDK_GATEWAY_* credentials; run with --ignored"]
async fn live_orchestrator_end_to_end() {
    let Some(gateway) = gateway_from_env() else {
        eprintln!("SKIP: gateway env not set (see .env.example)");
        return;
    };

    // Real provider + model.
    let provider = Arc::new(
        ai_providers::openai_compat::OpenAiCompatProvider::new(
            ai_providers::openai_compat::OpenAiCompatConfig::new(
                gateway.provider_id.clone(),
                gateway.api_key.clone(),
                gateway.base_url.clone(),
            ),
        )
        .expect("provider builds"),
    );
    let model: Arc<dyn ai_core::Model> = provider
        .model(&gateway.primary_model)
        .expect("primary model resolves");

    // Orchestrator wiring: real planner, empty pool (growth via derive),
    // one worker system prompt.
    let planner: Arc<dyn Planner> = Arc::new(LlmPlanner::new(model.clone()));
    let diag_planner = Arc::clone(&planner);
    let registry = Arc::new(AgentRegistry::new());
    let base_agent = Arc::new(
        AgentBuilder::new(
            "orchestra-worker",
            "You are a precise worker agent. Execute the given task exactly; \
             be concise. When asked for arithmetic, compute it yourself.",
            model,
        )
        .build(),
    );
    let orch = Arc::new(Orchestrator::new(
        planner,
        registry,
        base_agent,
        Arc::new(QuestionMailbox::new()),
        OrchestratorConfig {
            max_parallel_leaves: 3,
            watchdog: WatchdogConfig {
                hard_deadline: Duration::from_secs(180),
                ..WatchdogConfig::default()
            },
            ..OrchestratorConfig::default()
        },
    ));

    let started = Instant::now();
    let handle = orch.submit(
        "Compute 17*23, then write exactly one two-line haiku that mentions \
         the result. Keep it short.",
    );
    let _ = handle;

    // Wait for the fleet to settle (generous bound: real LLM planning plus
    // per-leaf runs).
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let status = orch.status_report().await;
        let open = status
            .counts_by_status
            .get(&NodeStatus::Pending)
            .copied()
            .unwrap_or(0)
            + status
                .counts_by_status
                .get(&NodeStatus::InProgress)
                .copied()
                .unwrap_or(0);
        if open == 0 && status.active_runs == 0 && status.submissions > 0 {
            eprintln!(
                "settled after {:?}: {:?} escalated={:?} peak={}",
                started.elapsed(),
                status.counts_by_status,
                status.escalated,
                status.active_peak
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "orchestrator did not settle in 300s"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let status = orch.status_report().await;
    let completed = status
        .counts_by_status
        .get(&NodeStatus::Completed)
        .copied()
        .unwrap_or(0);
    if completed == 0 {
        // Diagnostics first: what did the pipeline actually do?
        let sub_status = handle.status().await;
        let events = orch.events().await;
        eprintln!(
            "DIAG: submission={sub_status:?} counts={:?} awaiting={} events={events:?}",
            status.counts_by_status, status.awaiting_answers
        );
        match diag_planner.assess("Compute 17*23.").await {
            Ok(v) => eprintln!("DIAG: direct assess OK: {v:?}"),
            Err(e) => eprintln!("DIAG: direct assess ERR: {e}"),
        }
    }
    assert!(completed >= 1, "expected at least one completed leaf");
    assert!(
        status
            .counts_by_status
            .get(&NodeStatus::Failed)
            .copied()
            .unwrap_or(0)
            == 0,
        "no leaf should fail"
    );

    let events = orch.events().await;
    eprintln!(
        "PASS: live orchestrator end-to-end — {} leaves completed, {} audit events, {:?}",
        completed,
        events.len(),
        started.elapsed()
    );

    orch.shutdown(Duration::from_secs(10)).await;
}
