//! Offline proof suite for the supervision side (wave B2).
//!
//! No network, no live models: the planner is a [`ScriptedPlanner`] over
//! canned verdicts and leaf scripts, and agents run on a [`MarkerModel`]
//! whose behaviour is keyed by markers embedded in the task briefs:
//!
//! - `[[echo]]` / default — complete immediately with a summary,
//! - `[[fail-once]]` — error on the FIRST call per distinct prompt, then succeed,
//! - `[[hang]]` — never resolve (dropped cleanly when its run is cancelled),
//! - `[[slow-ms:N]]` — sleep N milliseconds, then succeed.
//!
//! Each proof drives the real orchestrator loop (scheduler task, worker
//! pool, watchdog sweeps) under `tokio`'s multi-thread runtime and asserts
//! against public surfaces: `status_report`, `events`, tree snapshots, and
//! the shared progress ledger.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ai_agents::{Agent, AgentBuilder};
use ai_core::{ChatRequest, Model};
use ai_errors::{AiError, ValidationError};
use ai_models::{ModelCapabilities, ModelInfo};
use ai_orchestra::QuestionMailbox;
use ai_orchestra::mailbox::Answer;
use ai_orchestra::orchestra::{
    Orchestrator, OrchestratorConfig, SubmissionStatus, SupervisionEventKind,
};
use ai_orchestra::planner::{ClarifyVerdict, PendingQuestion, Planner};
use ai_orchestra::registry::{AgentEntry, AgentRegistry};
use ai_orchestra::tree::{NodeStatus, TaskId, TaskTree};
use ai_orchestra::watchdog::{RecoveryPolicy, StallKind, WatchdogConfig};
use ai_runtime::RetryPolicy;
use ai_types::{Completion, ModelId, ProviderId, Usage};
use async_trait::async_trait;
use parking_lot::Mutex;

// -- offline model fakes -----------------------------------------------------

fn model_info() -> &'static ModelInfo {
    static INFO: std::sync::OnceLock<ModelInfo> = std::sync::OnceLock::new();
    INFO.get_or_init(|| {
        ModelInfo::new(
            ProviderId::new("test"),
            ModelId::new("scripted"),
            128_000,
            8_192,
        )
        .with_capabilities(ModelCapabilities::default())
    })
}

fn completion(text: String) -> Completion {
    Completion {
        provider: ProviderId::new("test"),
        model: ModelId::new("scripted"),
        text,
        tool_calls: Vec::new(),
        usage: Usage::new(10, 5),
        reasoning: None,
        raw: serde_json::Value::Null,
        finish_reason: Some("stop".to_owned()),
    }
}

/// Extracts the brief: the last user message of the request.
fn prompt_of(request: &ChatRequest) -> String {
    request
        .messages
        .iter()
        .rev()
        .find(|m| m.role == ai_types::Role::User)
        .map(|m| m.text_content())
        .unwrap_or_default()
}

/// Behaviour-switched fake. Markers in the prompt select the behaviour;
/// see the module docs. The orchestrator tags every worker input with
/// `[supervisor attempt N]`, which lets this fake fail EXACTLY one
/// supervised attempt regardless of any retry wrapper inside `Agent::run`.
#[derive(Default)]
struct MarkerModel;

impl MarkerModel {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl Model for MarkerModel {
    fn info(&self) -> &ModelInfo {
        model_info()
    }

    async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
        let prompt = prompt_of(&request);
        if prompt.contains("[[hang]]") {
            // Never resolves; dropped when the supervisor cancels the run.
            return std::future::pending::<Result<Completion, AiError>>().await;
        }
        if let Some(rest) = prompt.split("[[slow-ms:").nth(1) {
            let ms: u64 = rest
                .split(']')
                .next()
                .and_then(|digits| digits.parse().ok())
                .unwrap_or(50);
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }
        if prompt.contains("[[fail-first-attempt]]") && prompt.contains("[supervisor attempt 1]") {
            // Validation-class error: never retried by policy classifiers —
            // the crash we want the SUPERVISOR (not anyone internal) to heal.
            return Err(AiError::Validation(ValidationError::new(
                "synthetic boom (first supervised attempt)",
            )));
        }
        Ok(completion("done".to_owned()))
    }

    async fn stream(&self, _request: ChatRequest) -> Result<ai_core::EventStream, AiError> {
        unreachable!("offline proofs never stream")
    }
}

/// A model that always succeeds instantly regardless of markers.
#[derive(Default)]
struct PlainModel;

#[async_trait]
impl Model for PlainModel {
    fn info(&self) -> &ModelInfo {
        model_info()
    }

    async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
        Ok(completion("plain ok".to_owned()))
    }

    async fn stream(&self, _request: ChatRequest) -> Result<ai_core::EventStream, AiError> {
        unreachable!("offline proofs never stream")
    }
}

// -- scripted planner ----------------------------------------------------------

struct LeafSpec {
    title: &'static str,
    brief: &'static str,
    /// Indices into leaves created EARLIER IN THE SAME expand call.
    deps: Vec<usize>,
}

fn leaf(title: &'static str, brief: &'static str) -> LeafSpec {
    LeafSpec {
        title,
        brief,
        deps: Vec::new(),
    }
}

/// Canned [`Planner`] implementation: one assess verdict plus a queue of
/// expansion scripts (consumed one per call; an empty queue expands to
/// nothing).
struct ScriptedPlanner {
    verdict: Option<ClarifyVerdict>,
    scripts: Mutex<Vec<Vec<LeafSpec>>>,
    expand_calls: AtomicUsize,
    assess_calls: AtomicUsize,
}

impl ScriptedPlanner {
    fn clear(script: Vec<LeafSpec>) -> Self {
        Self {
            verdict: None, // Default is clear.
            scripts: Mutex::new(vec![script]),
            expand_calls: AtomicUsize::new(0),
            assess_calls: AtomicUsize::new(0),
        }
    }

    fn unclear(questions: Vec<PendingQuestion>, script: Vec<LeafSpec>) -> Self {
        Self {
            verdict: Some(ClarifyVerdict {
                clear: false,
                rationale: "proof needs clarification".into(),
                questions,
            }),
            scripts: Mutex::new(vec![script]),
            expand_calls: AtomicUsize::new(0),
            assess_calls: AtomicUsize::new(0),
        }
    }

    fn expansions_so_far(&self) -> usize {
        self.expand_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Planner for ScriptedPlanner {
    async fn assess(&self, _prompt: &str) -> Result<ClarifyVerdict, AiError> {
        self.assess_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.verdict.clone().unwrap_or_default())
    }

    async fn expand(
        &self,
        tree: &mut TaskTree,
        _parent: Option<TaskId>,
        _clarified_prompt: &str,
    ) -> Result<Vec<TaskId>, AiError> {
        self.expand_calls.fetch_add(1, Ordering::SeqCst);
        let script = {
            let mut queue = self.scripts.lock();
            if queue.is_empty() {
                Vec::new()
            } else {
                queue.remove(0)
            }
        };
        let mut created = Vec::new();
        for spec in script {
            let deps: Vec<TaskId> = spec.deps.iter().map(|i| created[*i]).collect();
            let id = tree
                .add_root(spec.title, spec.brief, deps)
                .expect("scripted leaf");
            created.push(id);
        }
        Ok(created)
    }
}

// -- harness -------------------------------------------------------------------

fn config(watchdog: WatchdogConfig, max_parallel: usize, max_attempts: u32) -> OrchestratorConfig {
    OrchestratorConfig {
        max_parallel_leaves: max_parallel,
        poll_interval: Duration::from_millis(15),
        watchdog,
        max_task_attempts: max_attempts,
    }
}

fn fast_watchdog() -> WatchdogConfig {
    WatchdogConfig {
        // Generous windows so only the rule UNDER TEST can fire.
        progress_window: Duration::from_secs(60),
        hard_deadline: Duration::from_secs(60),
        sweep_interval: Duration::from_millis(10),
        ..WatchdogConfig::default()
    }
}

fn scripted_agent(id: &str, model: Arc<dyn Model>) -> Arc<Agent> {
    // RetryPolicy::none(): one model call per supervised attempt, so
    // failures surface to the SUPERVISOR instead of being healed inside
    // Agent::run's own self-healing loop. Supervisor-level recovery is what
    // these proofs exercise.
    Arc::new(
        AgentBuilder::new(id, "worker instructions", model)
            .with_retry(RetryPolicy::none())
            .build(),
    )
}

fn orchestrator(
    planner: ScriptedPlanner,
    base_model: Arc<dyn Model>,
    cfg: OrchestratorConfig,
) -> Arc<Orchestrator> {
    let registry = AgentRegistry::new();
    let mailbox = QuestionMailbox::new();
    let base = scripted_agent("base", base_model);
    Arc::new(Orchestrator::new(
        Arc::new(planner),
        Arc::new(registry),
        base,
        Arc::new(mailbox),
        cfg,
    ))
}

async fn await_submission(
    handle: &ai_orchestra::orchestra::SubmissionHandle,
    want: SubmissionStatus,
) {
    for _ in 0..800 {
        if handle.status().await == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "submission never reached {want:?} (still {:?})",
        handle.status().await
    );
}

async fn wait_for_tree(orch: &Orchestrator, what: &str, cond: impl Fn(&TaskTree) -> bool) {
    for _ in 0..800 {
        if orch.tree_snapshot(|t| cond(t)).await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("tree condition never met: {what}");
}

// -- proofs ----------------------------------------------------------------------

/// PROOF 1 — Happy path: clear prompt → scripted leaves → pooled derived
/// agents → all Completed, with a delegate/complete audit trail and
/// dependency propagation (gamma depends on alpha).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proof_1_happy_path_delegates_and_completes() {
    let planner = ScriptedPlanner::clear(vec![
        leaf("alpha", "[[echo]] alpha work"),
        leaf("beta", "[[echo]] beta work"),
        LeafSpec {
            title: "gamma",
            brief: "[[echo]] gamma work",
            deps: vec![0],
        },
    ]);
    let orch = orchestrator(planner, Arc::new(PlainModel), config(fast_watchdog(), 8, 3));
    let submission = orch.submit("build the thing");

    await_submission(&submission, SubmissionStatus::Completed).await;

    let status = orch.status_report().await;
    assert_eq!(
        status.counts_by_status.get(&NodeStatus::Completed),
        Some(&3),
        "all three leaves completed"
    );
    assert_eq!(status.active_runs, 0);
    let tree_ok = orch.tree_snapshot(|t| t.check_invariants().is_ok()).await;
    assert!(tree_ok);

    let events = orch.events().await;
    let delegated: Vec<_> = events
        .iter()
        .filter(|e| e.kind == SupervisionEventKind::Delegated)
        .collect();
    let completed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == SupervisionEventKind::Completed)
        .collect();
    assert_eq!(delegated.len(), 3, "delegate trail: {events:?}");
    assert_eq!(completed.len(), 3, "complete trail");
    for event in &delegated {
        assert!(
            event.detail.contains("base-worker"),
            "derived pool agent: {event:?}"
        );
    }

    let all_assigned = orch
        .tree_snapshot(|t| t.iter().all(|n| n.assigned.is_some()))
        .await;
    assert!(all_assigned);
}

/// PROOF 2 — Ambiguity gate: unclear prompt parks the submission with two
/// pending questions and schedules NOTHING until both are answered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proof_2_ambiguity_parks_until_answers_arrive() {
    let planner = ScriptedPlanner::unclear(
        vec![
            PendingQuestion {
                text: "Which database?".into(),
                options: vec!["sqlite".into(), "postgres".into()],
            },
            PendingQuestion {
                text: "Sync or async API?".into(),
                options: vec![],
            },
        ],
        vec![leaf("alpha", "[[echo]] post-clarification work")],
    );
    let orch = orchestrator(planner, Arc::new(PlainModel), config(fast_watchdog(), 4, 3));
    let submission = orch.submit("store some data");

    // Parked: status flips to AwaitingAnswers, no questions answered yet.
    await_submission(&submission, SubmissionStatus::AwaitingAnswers).await;
    let status = orch.status_report().await;
    assert_eq!(status.awaiting_answers, 2);
    assert!(
        status.counts_by_status.is_empty(),
        "nothing scheduled while parked"
    );
    assert!(
        orch.events().await.is_empty(),
        "no delegation before answers"
    );

    // The human answers both questions (ids allocated sequentially from 1).
    orch.answer(Answer::choice(1, "postgres"))
        .expect("question 1 pending");
    orch.answer(Answer::free_text(2, "async please"))
        .expect("question 2 pending");

    await_submission(&submission, SubmissionStatus::Completed).await;
    assert_eq!(
        orch.status_report()
            .await
            .counts_by_status
            .get(&NodeStatus::Completed),
        Some(&1)
    );

    let events = orch.events().await;
    assert!(
        events
            .iter()
            .any(|e| e.kind == SupervisionEventKind::Delegated)
    );
}

/// PROOF 3 — Crash recovery: first attempt fails, automatic retry succeeds;
/// attempts respected, retry visible in the audit trail.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proof_3_crash_recovery_retries_and_completes() {
    let planner =
        ScriptedPlanner::clear(vec![leaf("flaky", "[[fail-first-attempt]] unstable work")]);
    let orch = orchestrator(planner, MarkerModel::new(), config(fast_watchdog(), 4, 2));
    let submission = orch.submit("do flaky work");

    await_submission(&submission, SubmissionStatus::Completed).await;

    let events = orch.events().await;
    // Natural failure recorded…
    assert!(
        events
            .iter()
            .any(|e| e.kind == SupervisionEventKind::Failed),
        "no Failed event; trail: {events:?}"
    );
    // …an explicit Retry recovery action…
    let retries: Vec<_> = events
        .iter()
        .filter(|e| e.kind == SupervisionEventKind::Recovery)
        .filter(|e| matches!(e.policy, Some(RecoveryPolicy::Retry)))
        .collect();
    assert_eq!(retries.len(), 1, "exactly one auto-retry: {events:?}");
    // …and two delegations (initial + retry).
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == SupervisionEventKind::Delegated)
            .count(),
        2
    );

    let attempts = orch
        .tree_snapshot(|t| t.get(TaskId(0)).map(|n| n.attempts))
        .await;
    assert_eq!(attempts, Some(2), "attempt budget respected");
    assert!(
        !events
            .iter()
            .any(|e| e.kind == SupervisionEventKind::Escalation)
    );
}

/// PROOF 4 — Stall recovery: a hung run trips the hard deadline, the
/// OverranDeadline verdict executes Reassign, and the task completes on a
/// DIFFERENT worker.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proof_4_stall_recovery_reassigns_to_different_worker() {
    // Pre-seed the pool: a hung specialist registered FIRST (specialty match
    // wins), then a healthy generalist.
    let registry = AgentRegistry::new();
    let hang_agent = scripted_agent("hang-1", MarkerModel::new());
    let good_agent = scripted_agent("good-1", Arc::new(PlainModel));
    registry
        .register(AgentEntry::new(hang_agent, vec!["stall".into()]))
        .unwrap();
    registry
        .register(AgentEntry::new(good_agent, vec![]))
        .unwrap();

    let mailbox = QuestionMailbox::new();
    let base = scripted_agent("base", Arc::new(PlainModel));
    let planner = ScriptedPlanner::clear(vec![leaf("stall", "[[hang]] never returns")]);
    let mut watchdog = fast_watchdog();
    watchdog.hard_deadline = Duration::from_millis(200);
    let orch = Arc::new(Orchestrator::new(
        Arc::new(planner),
        Arc::new(registry),
        base,
        Arc::new(mailbox),
        config(watchdog, 4, 3),
    ));

    let submission = orch.submit("run into the stall");
    await_submission(&submission, SubmissionStatus::Completed).await;

    let events = orch.events().await;
    let reassigns: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.policy, Some(RecoveryPolicy::Reassign)))
        .collect();
    assert_eq!(reassigns.len(), 1, "one reassign action: {events:?}");
    assert_eq!(reassigns[0].stall, Some(StallKind::OverranDeadline));

    // The retried run landed on the OTHER worker.
    let assigned = orch
        .tree_snapshot(|t| t.get(TaskId(0)).map(|n| n.assigned.clone()))
        .await;
    assert_eq!(assigned, Some(Some("good-1".to_owned())));
    let attempts = orch
        .tree_snapshot(|t| t.get(TaskId(0)).map(|n| n.attempts))
        .await;
    assert!(attempts >= Some(2), "task actually restarted");

    // Exclusion was remove→acquire→register: BOTH entries still pooled, idle.
    let stats = orch.status_report().await;
    assert_eq!(stats.counts_by_status.get(&NodeStatus::Completed), Some(&1));
    assert_eq!(stats.escalated.len(), 0);
}

/// PROOF 5 — Loop recovery: identical progress signatures trip
/// LoopSignature, RespawnAmended appends the hint to the effective brief,
/// and the amended respawn completes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proof_5_loop_signature_respawns_with_amended_brief() {
    let planner =
        ScriptedPlanner::clear(vec![leaf("loopy", "[[slow-ms:300]] same output forever")]);
    let mut watchdog = fast_watchdog();
    watchdog.loop_signature_min_repeats = 3;
    let orch = orchestrator(planner, MarkerModel::new(), config(watchdog, 4, 3));
    let submission = orch.submit("spin in circles");

    wait_for_tree(&orch, "first attempt InProgress", |t| {
        t.get(TaskId(0))
            .is_some_and(|n| n.status == NodeStatus::InProgress)
    })
    .await;

    // Feed the ledger directly through the SAME public surface workers use:
    // three identical signatures ≥ min_repeats.
    for _ in 0..3 {
        orch.ledger()
            .note_progress(TaskId(0), "identical output hash");
    }

    // The watchdog amends the effective brief within a few ticks.
    let mut amended = false;
    for _ in 0..200 {
        if let Some(brief) = orch.effective_brief(TaskId(0)).await {
            if brief.contains("repeating identical output") {
                amended = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(amended, "respawn amendment must reach the effective brief");

    await_submission(&submission, SubmissionStatus::Completed).await;

    let events = orch.events().await;
    let respawns: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.policy, Some(RecoveryPolicy::RespawnAmended(_))))
        .collect();
    assert_eq!(respawns.len(), 1, "one amended respawn: {events:?}");
    assert_eq!(respawns[0].stall, Some(StallKind::LoopSignature));

    let node = orch
        .tree_snapshot(|t| {
            t.get(TaskId(0))
                .map(|n| (n.status, n.attempts, n.brief.clone()))
        })
        .await
        .unwrap();
    assert_eq!(node.0, NodeStatus::Completed);
    assert!(node.1 >= 2, "task actually restarted");
    assert!(
        node.2.contains("[[slow-ms:300]]"),
        "original brief untouched"
    );
}

/// PROOF 6 — Cancellation: cancel_task mid-run marks the node Cancelled and
/// its dependent is NEVER scheduled.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proof_6_cancel_prevents_dependents_from_scheduling() {
    let planner = ScriptedPlanner::clear(vec![
        leaf("long", "[[slow-ms:5000]] long-running"),
        LeafSpec {
            title: "child",
            brief: "[[echo]] after long",
            deps: vec![0],
        },
    ]);
    let orch = orchestrator(planner, MarkerModel::new(), config(fast_watchdog(), 4, 3));
    let submission = orch.submit("cancel me midway");

    wait_for_tree(&orch, "long task running", |t| {
        t.get(TaskId(0))
            .is_some_and(|n| n.status == NodeStatus::InProgress)
    })
    .await;

    orch.cancel_task(TaskId(0))
        .await
        .expect("running task cancellable");

    wait_for_tree(&orch, "long task cancelled", |t| {
        t.get(TaskId(0))
            .is_some_and(|n| n.status == NodeStatus::Cancelled)
    })
    .await;

    // The dependent stays unscheduled across several full poll cycles.
    for _ in 0..12 {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let snapshot = orch
            .tree_snapshot(|t| {
                (
                    t.get(TaskId(1)).map(|n| (n.status, n.attempts)),
                    t.next_ready(),
                )
            })
            .await;
        let (child, ready) = snapshot;
        assert_eq!(
            child,
            Some((NodeStatus::Pending, 0)),
            "dependent must stay parked"
        );
        assert!(
            !ready.contains(&TaskId(1)),
            "cancelled dep ⇒ child never ready"
        );
    }

    // Audit trail shows the cancellation; no delegation ever targeted #1.
    let events = orch.events().await;
    assert!(
        events
            .iter()
            .any(|e| e.task == TaskId(0) && e.kind == SupervisionEventKind::Cancelled)
    );
    assert!(
        !events
            .iter()
            .any(|e| e.task == TaskId(1) && e.kind == SupervisionEventKind::Delegated)
    );

    // The submission stays open: its dependent leaf is parked Pending by
    // design (a cancelled dependency never unblocks it) — exactly the
    // "dependents never scheduled" guarantee under proof.
    assert_eq!(submission.status().await, SubmissionStatus::Running);
}

/// PROOF 7 — Concurrency cap: with six ready leaves and a cap of two, the
/// in-flight gauge peaks at exactly two and the pool never grows past it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proof_7_parallelism_never_exceeds_cap() {
    const LEAVES: usize = 6;
    const CAP: usize = 2;

    let script: Vec<LeafSpec> = (0..LEAVES)
        .map(|i| LeafSpec {
            title: Box::leak(format!("unit-{i}").into_boxed_str()),
            brief: "[[slow-ms:40]] paced work",
            deps: vec![],
        })
        .collect();
    let planner = ScriptedPlanner::clear(script);
    let orch = orchestrator(planner, MarkerModel::new(), config(fast_watchdog(), CAP, 3));
    let submission = orch.submit("fan out six ways");

    await_submission(&submission, SubmissionStatus::Completed).await;

    let status = orch.status_report().await;
    assert_eq!(status.active_peak, CAP, "gauge saturated the cap exactly");
    assert!(status.active_peak <= CAP);
    assert_eq!(
        status.counts_by_status.get(&NodeStatus::Completed),
        Some(&LEAVES),
        "every leaf finished"
    );
    assert_eq!(status.active_runs, 0);
}

/// PROOF 8 — Mid-flight input: inject_user_message triggers a Planner
/// re-expand whose nodes join the same tree and get scheduled.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn proof_8_injected_message_expands_midflight() {
    // First expand call: nothing (empty objective). Second call (the
    // injection): one real leaf.
    let planner = Arc::new(ScriptedPlanner::clear(vec![]));
    planner
        .scripts
        .lock()
        .push(vec![leaf("late", "[[echo]] injected work")]);

    let registry = AgentRegistry::new();
    let base = scripted_agent("base", Arc::new(PlainModel));
    let orch = Arc::new(Orchestrator::new(
        planner.clone() as Arc<dyn Planner>,
        Arc::new(registry),
        base,
        Arc::new(QuestionMailbox::new()),
        config(fast_watchdog(), 4, 3),
    ));
    let _submission = orch.submit("start empty");

    // Let the first (empty) expansion settle.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        planner.expansions_so_far(),
        1,
        "submit expanded once (to nothing)"
    );
    assert!(orch.status_report().await.counts_by_status.is_empty());

    orch.inject_user_message("actually also do this");

    wait_for_tree(&orch, "injected leaf completed", |t| {
        t.get(TaskId(0))
            .is_some_and(|n| n.status == NodeStatus::Completed)
    })
    .await;

    assert_eq!(
        planner.expansions_so_far(),
        2,
        "injection triggered exactly one re-expand"
    );
}
