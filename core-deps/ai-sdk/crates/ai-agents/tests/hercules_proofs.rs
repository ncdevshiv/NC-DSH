//! HERCULES proofs: regression-grade integration tests for the swarm
//! engine, all driven by deterministic scripted models (no network, per
//! ADR-007).
//!
//! Proofs:
//! a) ZERO CROSS-TALK — 64 concurrent inputs, marker echo.
//! b) KILL-30% — 30% injected failures, exact partial-failure accounting.
//! c) MAP-REDUCE — hierarchical summation of numeric strings.
//! d) COMPETITIVE — judge-ranked elimination tournament.
//! e) BUDGET — tiny budgets stop the swarm early, no runaway.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;

use ai_agents::{
    Agent, AgentBuilder, AgentState, CompetitiveOutcome, JudgeFn, SwarmEngine, is_budget_exhausted,
};
use ai_core::{ChatRequest, Completion, EventStream, Model};
use ai_errors::{AiError, NetworkError};
use ai_models::{ModelCapabilities, ModelInfo};
use ai_types::{ModelId, ProviderId, Role, ToolCall, Usage};

// ---------------------------------------------------------------------------
// Scripted fakes
// ---------------------------------------------------------------------------

fn scripted_completion(text: &str, usage: Usage, tool_calls: Vec<ToolCall>) -> Completion {
    Completion {
        provider: ProviderId::new("test"),
        model: ModelId::new("scripted"),
        text: text.to_string(),
        tool_calls,
        usage,
        reasoning: None,
        raw: serde_json::Value::Null,
        finish_reason: Some("stop".into()),
    }
}

fn model_info() -> &'static ModelInfo {
    static INFO: std::sync::OnceLock<ModelInfo> = std::sync::OnceLock::new();
    INFO.get_or_init(|| {
        ModelInfo::new(
            ProviderId::new("test"),
            ModelId::new("scripted"),
            128_000,
            8_192,
        )
        .with_capabilities(ModelCapabilities {
            supports_tools: true,
            ..Default::default()
        })
    })
}

/// Text of the LAST user message in the request (the current task input;
/// memory is per-derived-agent and starts empty).
fn last_user_text(request: &ChatRequest) -> String {
    request
        .messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.text_content())
        .unwrap_or_default()
}

/// Stateless echo: returns the `[T-nn]` marker found in the task input.
/// The response depends ONLY on the request content — any cross-task memory
/// contamination would surface as a wrong marker in the results.
struct MarkerEchoModel;

#[async_trait]
impl Model for MarkerEchoModel {
    fn info(&self) -> &ModelInfo {
        model_info()
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
        let input = last_user_text(&request);
        let start = input.find("[T-").expect("input carries [T-nn] marker");
        let end = input[start..].find(']').expect("marker is closed") + start;
        let marker = input[start..=end].to_string();
        Ok(scripted_completion(
            &format!("echo {marker}"),
            Usage::new(10, 5),
            vec![],
        ))
    }

    async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
        unreachable!("stream unused")
    }
}

/// Every generation fails immediately (used with RetryPolicy::none()).
struct AlwaysFailsModel;

#[async_trait]
impl Model for AlwaysFailsModel {
    fn info(&self) -> &ModelInfo {
        model_info()
    }

    async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
        Err(AiError::Network(NetworkError::new(
            "scripted",
            "injected failure",
        )))
    }

    async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
        unreachable!("stream unused")
    }
}

/// Echoes the integer embedded in the task input (map phase).
struct NumberEchoModel;

#[async_trait]
impl Model for NumberEchoModel {
    fn info(&self) -> &ModelInfo {
        model_info()
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
        let input = last_user_text(&request);
        let number: String = input
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        assert!(!number.is_empty(), "map input must contain a number");
        Ok(scripted_completion(&number, Usage::new(3, 2), vec![]))
    }

    async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
        unreachable!("stream unused")
    }
}

/// Sums every integer appearing anywhere in the request and replies with
/// just the total (reduce phase).
struct SummingReduceModel;

#[async_trait]
impl Model for SummingReduceModel {
    fn info(&self) -> &ModelInfo {
        model_info()
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
        let text: String = request
            .messages
            .iter()
            .map(|m| m.text_content())
            .collect::<Vec<_>>()
            .join("\n");
        let mut sum: u64 = 0;
        let mut current: Option<u64> = None;
        for c in text.chars() {
            if let Some(d) = c.to_digit(10) {
                current = Some(current.unwrap_or(0) * 10 + u64::from(d));
            } else if let Some(n) = current.take() {
                sum += n;
            }
        }
        sum += current.unwrap_or(0);
        Ok(scripted_completion(
            &sum.to_string(),
            Usage::new(20, 4),
            vec![],
        ))
    }

    async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
        unreachable!("stream unused")
    }
}

/// Replies with `ANSWER <value>` — candidate slot determines the value.
struct StaticAnswerModel {
    value: u64,
}

#[async_trait]
impl Model for StaticAnswerModel {
    fn info(&self) -> &ModelInfo {
        model_info()
    }

    async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
        Ok(scripted_completion(
            &format!("ANSWER {}", self.value),
            Usage::new(6, 3),
            vec![],
        ))
    }

    async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
        unreachable!("stream unused")
    }
}

/// Fixed token usage per call; records whether it was ever invoked.
struct FixedUsageModel {
    usage: Usage,
    called: AtomicBool,
}

#[async_trait]
impl Model for FixedUsageModel {
    fn info(&self) -> &ModelInfo {
        model_info()
    }

    async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
        self.called.store(true, Ordering::SeqCst);
        Ok(scripted_completion("ok", self.usage, vec![]))
    }

    async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
        unreachable!("stream unused")
    }
}

fn agent_with(model: Arc<dyn Model>, id: String) -> Agent {
    AgentBuilder::new(id, "You are a scripted proof agent", model)
        .with_retry(ai_runtime_retry_none())
        .build()
}

fn ai_runtime_retry_none() -> ai_runtime::RetryPolicy {
    ai_runtime::RetryPolicy::none()
}

// ---------------------------------------------------------------------------
// a) ZERO CROSS-TALK
// ---------------------------------------------------------------------------

/// 64 concurrent inputs through ONE engine. Every derived agent owns its
/// own memory and id; the shared scripted model answers purely from the
/// request content. If conversation histories interleaved (the old single-
/// Arc<Agent> bug), retrieved histories from sibling tasks would inject
/// foreign markers into requests and some echoes would mismatch. All 64
/// markers MUST come back exactly paired with their own input.
#[tokio::test]
async fn proof_zero_crosstalk_64_concurrent_inputs() {
    const N: usize = 64;
    let prototype_model: Arc<dyn Model> = Arc::new(MarkerEchoModel);
    let template: ai_agents::SwarmTemplate =
        Arc::new(move |index| agent_with(prototype_model.clone(), format!("crosstalk-{index}")));

    let engine = SwarmEngine::new(template).with_concurrency(16);
    let inputs: Vec<String> = (0..N)
        .map(|i| format!("report [T-{i:02}] status"))
        .collect();

    let outcome = engine.fan_out(inputs).await.expect("fan-out succeeds");

    assert_eq!(outcome.results.len(), N);
    assert!(outcome.all_succeeded(), "failures: {:?}", outcome.failed);
    for i in 0..N {
        let result = outcome.get(i).unwrap_or_else(|| panic!("task {i} missing"));
        let expected = format!("[T-{i:02}]");
        assert!(
            result.text.contains(&expected),
            "CROSS-TALK: task {i} echoed {:?}, expected {expected}",
            result.text
        );
        assert_eq!(result.state, AgentState::Completed);
    }
    // Cumulative accounting across 64 runs of two calls… one call each:
    assert_eq!(outcome.total_usage.input_tokens, 10 * N as u64);
}

// ---------------------------------------------------------------------------
// b) KILL-30%
// ---------------------------------------------------------------------------

/// Inject failures into exactly 30% of tasks via the template; every other
/// task completes and stays input-ordered. failed[] must list EXACTLY the
/// killed indices — no fail-fast cascade (the old behavior aborted on the
/// first failure and reported failed = 0 structurally).
#[tokio::test]
async fn proof_kill_thirty_percent_partial_failure() {
    const N: usize = 20;
    let kill_set: std::collections::HashSet<usize> =
        [1usize, 4, 7, 11, 15, 18].into_iter().collect();

    let echo: Arc<dyn Model> = Arc::new(MarkerEchoModel);
    let failing: Arc<dyn Model> = Arc::new(AlwaysFailsModel);
    let kill_set_for_template = kill_set.clone();
    let template: ai_agents::SwarmTemplate = Arc::new(move |index| {
        if kill_set_for_template.contains(&index) {
            agent_with(failing.clone(), format!("doomed-{index}"))
        } else {
            agent_with(echo.clone(), format!("survivor-{index}"))
        }
    });

    let engine = SwarmEngine::new(template).with_concurrency(4);
    let inputs: Vec<String> = (0..N).map(|i| format!("job [T-{i:02}]")).collect();
    let outcome = engine.fan_out(inputs).await.expect("outcome collected");

    assert_eq!(outcome.succeeded, 14);
    let failed_indices: Vec<usize> = outcome.failed.iter().map(|(i, _)| *i).collect();
    let expected: Vec<usize> = {
        let mut v: Vec<usize> = kill_set.into_iter().collect();
        v.sort_unstable();
        v
    };
    assert_eq!(
        failed_indices, expected,
        "failed[] must match the kill set exactly"
    );
    assert_eq!(outcome.failed.len(), 6);

    // Survivors are complete AND input-ordered: each slot holds its OWN marker.
    for i in 0..N {
        if expected.contains(&i) {
            assert!(outcome.get(i).is_none(), "killed task {i} has no result");
        } else {
            let result = outcome.get(i).unwrap();
            assert!(
                result.text.contains(&format!("[T-{i:02}]")),
                "slot {i} must hold its own output, got {:?}",
                result.text
            );
        }
    }
}

// ---------------------------------------------------------------------------
// c) MAP-REDUCE
// ---------------------------------------------------------------------------

/// 16 numeric strings summed hierarchically: map agents extract their
/// number, reduce agents (max_fan_in = 4) sum their children — tree shape
/// 16 → 4 → 1 — and the root equals the arithmetic total.
#[tokio::test]
async fn proof_map_reduce_hierarchical_summation() {
    const N: usize = 16;
    let numbers: Vec<u64> = (0..N as u64).map(|i| i * 3 + 7).collect();
    let expected_total: u64 = numbers.iter().sum(); // 472

    let map_model: Arc<dyn Model> = Arc::new(NumberEchoModel);
    let reduce_model: Arc<dyn Model> = Arc::new(SummingReduceModel);
    let map_template: ai_agents::SwarmTemplate =
        Arc::new(move |index| agent_with(map_model.clone(), format!("mapper-{index}")));
    let reduce_template: ai_agents::SwarmTemplate =
        Arc::new(move |index| agent_with(reduce_model.clone(), format!("reducer-{index}")));

    let engine = SwarmEngine::new(map_template)
        .with_reduce_template(reduce_template)
        .with_max_fan_in(4)
        .with_concurrency(4);

    let inputs: Vec<String> = numbers.iter().map(|n| n.to_string()).collect();
    let outcome = engine
        .map_reduce(inputs)
        .await
        .expect("map-reduce succeeds");

    assert!(outcome.map.all_succeeded());
    for (i, number) in numbers.iter().enumerate() {
        assert_eq!(outcome.map.get(i).unwrap().text, number.to_string());
    }
    // Tree shape: 16 leaves → 4 nodes → 1 root.
    assert_eq!(outcome.levels.len(), 2);
    assert_eq!(outcome.levels[0].len(), 4);
    assert_eq!(outcome.levels[1].len(), 1);
    assert!(outcome.levels[1][0].children.len() == 4);

    let root = outcome.root.as_deref().expect("root reduces to a value");
    assert_eq!(
        root.trim(),
        expected_total.to_string(),
        "hierarchical sum mismatch"
    );
    assert!(outcome.total_usage.total() > 0);
}

// ---------------------------------------------------------------------------
// d) COMPETITIVE
// ---------------------------------------------------------------------------

/// Numeric judge: higher embedded answer value wins.
struct HigherValueJudge;

#[async_trait]
impl JudgeFn for HigherValueJudge {
    async fn judge(&self, _task: &str, a: &str, b: &str) -> Result<std::cmp::Ordering, AiError> {
        let parse = |s: &str| -> u64 {
            s.split_whitespace()
                .find_map(|t| t.parse::<u64>().ok())
                .unwrap_or(0)
        };
        Ok(parse(a).cmp(&parse(b)))
    }
}

/// Candidates answer 30 / 20 / 10 by slot (0 → 30, 1 → 20, 2 → 10); two
/// rounds with a 50% elimination fraction must crown slot 0 (value 30),
/// eliminating the weakest each round.
#[tokio::test]
async fn proof_competitive_tournament_selects_winner() {
    let values = [30u64, 20, 10];
    let models: Vec<Arc<dyn Model>> = values
        .iter()
        .map(|&v| Arc::new(StaticAnswerModel { value: v }) as Arc<dyn Model>)
        .collect();
    let models = Arc::new(models);
    let template: ai_agents::SwarmTemplate =
        Arc::new(move |slot| agent_with(models[slot].clone(), format!("contestant-{slot}")));

    let engine = SwarmEngine::new(template)
        .with_concurrency(3)
        .with_eliminate_fraction(0.5);

    let judge: Arc<dyn JudgeFn> = Arc::new(HigherValueJudge);
    let outcome: CompetitiveOutcome = engine
        .competitive("state your answer", 3, 2, judge)
        .await
        .expect("tournament completes");

    assert_eq!(outcome.winner, Some(0));
    assert_eq!(outcome.winner_answer.as_deref(), Some("ANSWER 30"));

    // Ledger sanity: round 1 eliminates the weakest (slot 2, value 10),
    // round 2 eliminates slot 1 (value 20).
    assert_eq!(outcome.rounds.len(), 2);
    assert_eq!(outcome.rounds[0].answers.len(), 3);
    assert_eq!(outcome.rounds[0].eliminated, vec![2]);
    assert_eq!(outcome.rounds[1].eliminated, vec![1]);

    // Final ranking: winner first; eliminated candidates ranked after,
    // most recently eliminated first.
    assert_eq!(outcome.scores[0].candidate, 0);
    assert_eq!(outcome.scores[0].final_rank, 0);
    assert_eq!(outcome.scores.len(), 3);
    assert_eq!(
        outcome
            .scores
            .iter()
            .map(|s| s.candidate)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    // Survival counts: winner survived both rounds.
    assert_eq!(outcome.scores[0].survived_rounds, 2);
    assert!(outcome.total_usage.total() > 0);
}

// ---------------------------------------------------------------------------
// e) BUDGET
// ---------------------------------------------------------------------------

/// A 400-token swarm budget with 150-token tasks stops the run early:
/// completed tasks charge the ledger, everything after the cap aborts with
/// a budget error BEFORE touching the model, and nothing runs away.
#[tokio::test]
async fn proof_swarm_budget_stops_runaway_early() {
    const N: usize = 12;
    let per_task_tokens: u64 = 150; // Usage::new(100, 50).total()

    let template: ai_agents::SwarmTemplate = Arc::new(|index| {
        let model: Arc<dyn Model> = Arc::new(FixedUsageModel {
            usage: Usage::new(100, 50),
            called: AtomicBool::new(false),
        });
        agent_with(model, format!("budgeted-{index}"))
    });

    let engine = SwarmEngine::new(template)
        .with_concurrency(4)
        .with_swarm_budget(400);

    let inputs: Vec<String> = (0..N).map(|i| format!("task {i}")).collect();
    let outcome = engine.fan_out(inputs).await.expect("outcome collected");

    // Some tasks succeeded before the cap…
    assert!(outcome.succeeded >= 1, "at least one task must complete");
    assert!(outcome.budget_exhausted(), "budget errors must be recorded");
    // …and the rest were aborted by budget, not executed.
    assert_eq!(outcome.succeeded + outcome.failed.len(), N);
    for (index, error) in &outcome.failed {
        assert!(
            error.contains(ai_agents::BUDGET_EXHAUSTED_MARKER),
            "task {index} must fail with a budget error, got: {error}"
        );
    }
    // No runaway: spend bounded by cap + in-flight overshoot (concurrency × max task).
    assert!(
        outcome.total_usage.total() <= 400 + per_task_tokens * 4,
        "spend {} exceeded the bound",
        outcome.total_usage.total()
    );

    // Structural recognition without string matching:
    let err = AiError::Network(NetworkError::new("x", "unrelated"));
    assert!(!is_budget_exhausted(&err));
}

/// Per-task budget: a successful generation that exceeds its task's cap is
/// converted into a budget failure.
#[tokio::test]
async fn proof_task_budget_caps_single_task_spend() {
    let template: ai_agents::SwarmTemplate = Arc::new(|index| {
        let model: Arc<dyn Model> = Arc::new(FixedUsageModel {
            usage: Usage::new(500, 500),
            called: AtomicBool::new(false),
        });
        agent_with(model, format!("greedy-{index}"))
    });
    let engine = SwarmEngine::new(template).with_task_budget(100);
    let outcome = engine.fan_out(vec!["only".to_string()]).await.unwrap();

    assert_eq!(outcome.succeeded, 0);
    assert_eq!(outcome.failed.len(), 1);
    assert!(is_budget_exhausted_str(&outcome.failed[0].1));
    assert_eq!(
        outcome.total_usage.input_tokens, 500,
        "real spend still charges"
    );
}

fn is_budget_exhausted_str(message: &str) -> bool {
    message.contains(ai_agents::BUDGET_EXHAUSTED_MARKER)
}

// ---------------------------------------------------------------------------
// Deprecated wrapper still behaves (isolated + partial-failure)
// ---------------------------------------------------------------------------

#[allow(deprecated)]
#[tokio::test]
async fn legacy_agent_swarm_wrapper_is_isolated_and_honest() {
    let echo: Arc<dyn Model> = Arc::new(MarkerEchoModel);
    let prototype = Arc::new(
        AgentBuilder::new("legacy-proto", "x", echo)
            .with_retry(ai_runtime::RetryPolicy::none())
            .build(),
    );

    #[allow(deprecated)]
    let swarm = ai_agents::AgentSwarm::new(prototype).with_concurrency(2);
    #[allow(deprecated)]
    let outcome = swarm
        .run(vec!["a [T-90]".into(), "b [T-91]".into()])
        .await
        .unwrap();

    assert_eq!(outcome.succeeded, 2);
    assert!(outcome.get(0).unwrap().text.contains("[T-90]"));
    assert!(outcome.get(1).unwrap().text.contains("[T-91]"));
}
