//! The orchestrator control loop: decompose through the [`Planner`] seam,
//! delegate tree leaves to pooled agents, watch every run, recover from
//! chaos, and accept mid-flight user input.
//!
//! # Loop design
//!
//! One long-lived scheduler task owns progress. Everything else feeds it:
//!
//! ```text
//! submit(prompt) ──▶ pipeline task ─┬─ assess (Planner)
//!                                   ├─ unclear ⇒ ask via mailbox,
//!                                   │            park AwaitingAnswers,
//!                                   │            resume on answer()
//!                                   └─ expand  ⇒ nodes into shared tree
//!                                                ⇒ Wake
//! inject_user_message ──▶ inbox ──▶ tick drains it (re-expand, v1: always)
//!
//! scheduler select! {
//!   tick          ⇒ drain inbox · watchdog sweep · apply recovery · fill slots
//!   Done(task,seq)⇒ retire run · apply outcome · propagate · fill slots
//!   Wake          ⇒ fill slots
//! }
//! ```
//!
//! Filling slots takes `next_ready()` leaves up to
//! [`OrchestratorConfig::max_parallel_leaves`], acquires a worker per leaf
//! (specialty = node title; empty pool grows by deriving from the base
//! agent — pool growth is this module's job), pairs a
//! [`RunHandle`](crate::handle::RunHandle), and spawns the worker task. The
//! worker notes start/end progress in the shared
//! [`ProgressLedger`], races `agent.run` against the cancellation token,
//! reports through its guard, then notifies the loop.
//!
//! # Supervision
//!
//! Every tick also sweeps active runs ([`Watchdog::sweep`]) and applies the
//! recommended [`RecoveryPolicy`]:
//!
//! - **Retry** / **RespawnAmended** — cancel the current run, release its
//!   worker, mark failed, retry (`Failed → Pending`); amended policies
//!   append a hint to the *effective brief* first. Attempts are respected:
//!   past [`OrchestratorConfig::max_task_attempts`] the task escalates to
//!   `Blocked` instead.
//! - **Reassign** — as Retry, plus the failed worker's id is excluded when
//!   re-acquiring. The registry has no exclusion parameter, so exclusion is
//!   implemented by `remove(excluded) → acquire → register(excluded)` — a
//!   documented limitation workaround using only public registry APIs.
//! - **Escalate** — park the task `Blocked`; surfaced in
//!   [`Orchestrator::status_report`].
//! - **Nudge** — audit-trail entry only (budget placeholder).
//!
//! Every delegation, completion, failure, cancellation, and recovery lands
//! in an append-only audit trail queryable via [`Orchestrator::events`].
//!
//! # Documented simplifications (v1)
//!
//! - All submissions share ONE growing tree; injected user messages are
//!   ALWAYS re-expanded under the same root set (no filtering heuristic).
//! - `Agent::run` exposes no iteration callback, so workers note progress
//!   once at start and once at end; loop detection is exercised honestly by
//!   tests seeding the same public ledger the workers use.
//! - Task-tree briefs are frozen (`TaskTree` offers no mutator), so
//!   amendment hints overlay onto the node brief via
//!   [`Orchestrator::effective_brief`] — what respawned workers actually
//!   receive.
//! - A pipeline whose planner call fails marks its submission `Failed`;
//!   questions retracted before answering resolve as "(retracted)" text.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use ai_agents::Agent;

use crate::handle::{RunGuard, RunHandle, TaskOutcome};
use crate::mailbox::{Answer, QuestionMailbox};
use crate::planner::Planner;
use crate::registry::{AgentRegistry, WorkerAdapter, derive_entry};
use crate::tree::{NodeStatus, TaskId, TaskTree, TreeError};
use crate::watchdog::{ProgressLedger, RecoveryPolicy, StallKind, Watchdog, WatchdogConfig};

// -- configuration ---------------------------------------------------------

/// Orchestrator tuning knobs.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Maximum concurrently running leaves. Default 8.
    pub max_parallel_leaves: usize,
    /// Scheduler tick period. Default 50ms.
    pub poll_interval: Duration,
    /// Stall-detection tuning handed to the [`Watchdog`].
    pub watchdog: WatchdogConfig,
    /// Times a task may START before escalation instead of retry. Default 3.
    pub max_task_attempts: u32,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_parallel_leaves: 8,
            poll_interval: Duration::from_millis(50),
            watchdog: WatchdogConfig::default(),
            max_task_attempts: 3,
        }
    }
}

// -- reporting types --------------------------------------------------------

/// Lifecycle of one submission as seen from the outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionStatus {
    /// Ambiguity assessment still running.
    Assessing,
    /// Parked until every clarifying question is answered.
    AwaitingAnswers,
    /// Expanded; at least one leaf still open.
    Running,
    /// Every leaf reached `Completed`.
    Completed,
    /// Finished with at least one non-success leaf (failed, cancelled, or
    /// blocked), or the planner itself errored.
    Failed,
}

/// Fleet-level snapshot for operators.
#[derive(Debug, Clone, Default)]
pub struct OrchestratorStatus {
    /// Node count grouped by [`NodeStatus`].
    pub counts_by_status: BTreeMap<NodeStatus, usize>,
    /// Clarification questions awaiting human answers.
    pub awaiting_answers: usize,
    /// Runs currently holding a worker.
    pub active_runs: usize,
    /// High-water mark of `active_runs` since construction.
    pub active_peak: usize,
    /// Tasks parked for human attention by escalation.
    pub escalated: Vec<TaskId>,
    /// Submissions seen so far.
    pub submissions: usize,
}

/// What kind of thing happened to a task (audit trail entries).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisionEventKind {
    /// A worker was paired with the task.
    Delegated,
    /// The task reached `Completed`.
    Completed,
    /// The task finished unsuccessfully.
    Failed,
    /// The task was cancelled.
    Cancelled,
    /// A recovery policy was applied.
    Recovery,
    /// Automatic retries were exhausted; parked for a human.
    Escalation,
}

/// Append-only audit trail entry; proofs assert against these.
#[derive(Debug, Clone)]
pub struct SupervisionEvent {
    /// Monotonic sequence across all events.
    pub seq: u64,
    /// Affected task.
    pub task: TaskId,
    /// What happened.
    pub kind: SupervisionEventKind,
    /// The recovery policy involved, if any.
    pub policy: Option<RecoveryPolicy>,
    /// Which stall kind triggered a recovery action, if any.
    pub stall: Option<StallKind>,
    /// Human-readable detail (error text, trigger description…).
    pub detail: String,
}

// -- internal state ----------------------------------------------------------

enum Phase {
    Assessing,
    ParkedAwaitingAnswers,
    Expanded,
    PlannerFailed,
}

struct SubmissionRecord {
    phase: Phase,
    leaves: Vec<TaskId>,
}

struct RunMeta {
    entry: Arc<dyn WorkerAdapter>,
    agent_id: String,
    seq: u64,
}

#[derive(Default)]
struct OrchestraState {
    tree: TaskTree,
    /// Supervisor-side live runs, keyed by task — exactly what
    /// [`Watchdog::sweep`] consumes.
    handles: HashMap<TaskId, RunHandle>,
    /// Per-run bookkeeping parallel to `handles`.
    meta: HashMap<TaskId, RunMeta>,
    /// Amendment hints overlaid onto node briefs (see module docs).
    amendments: HashMap<TaskId, Vec<String>>,
    /// Workers excluded from re-acquisition after a Reassign.
    excluded: HashMap<TaskId, String>,
    events: Vec<SupervisionEvent>,
    submissions: BTreeMap<u64, SubmissionRecord>,
    escalated: BTreeSet<TaskId>,
    active_runs: usize,
    active_peak: usize,
}

enum Msg {
    Done(TaskId, u64),
    Wake,
}

fn compose_brief(base_brief: &str, amendments: &[String]) -> String {
    let mut brief = base_brief.to_owned();
    for hint in amendments {
        brief.push_str("\n[supervisor amendment] ");
        brief.push_str(hint);
    }
    brief
}

// -- the orchestrator --------------------------------------------------------

/// The supervisor: owns the tree, the pool interaction, the watchdog sweep,
/// and the audit trail. Cheap to share — every method takes `&Arc<Self>`.
pub struct Orchestrator {
    planner: Arc<dyn Planner>,
    registry: Arc<AgentRegistry>,
    base_agent: Arc<Agent>,
    mailbox: Arc<QuestionMailbox>,
    config: OrchestratorConfig,
    watchdog: Watchdog,
    state: Arc<Mutex<OrchestraState>>,
    inbox: Arc<parking_lot::Mutex<Vec<String>>>,
    msg_tx: mpsc::UnboundedSender<Msg>,
    msg_rx: parking_lot::Mutex<Option<mpsc::UnboundedReceiver<Msg>>>,
    shutdown: CancellationToken,
    scheduler: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
    pipelines: Arc<parking_lot::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    next_submission_id: AtomicU64,
    next_question_id: AtomicU64,
    next_run_seq: AtomicU64,
    event_seq: AtomicU64,
    derive_counter: AtomicU64,
}

impl std::fmt::Debug for Orchestrator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orchestrator")
            .field("config", &self.config)
            .field(
                "submissions",
                &self.next_submission_id.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl Orchestrator {
    /// Wires the supervisor around the planning seam, the worker pool, a
    /// growth base agent, and the clarification mailbox.
    #[must_use]
    pub fn new(
        planner: Arc<dyn Planner>,
        registry: Arc<AgentRegistry>,
        base_agent: Arc<Agent>,
        mailbox: Arc<QuestionMailbox>,
        config: OrchestratorConfig,
    ) -> Self {
        let ledger = ProgressLedger::new();
        let watchdog = Watchdog::new(config.watchdog.clone(), ledger);
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        Self {
            planner,
            registry,
            base_agent,
            mailbox,
            config,
            watchdog,
            state: Arc::new(Mutex::new(OrchestraState::default())),
            inbox: Arc::new(parking_lot::Mutex::new(Vec::new())),
            msg_tx,
            msg_rx: parking_lot::Mutex::new(Some(msg_rx)),
            shutdown: CancellationToken::new(),
            scheduler: parking_lot::Mutex::new(None),
            pipelines: Arc::new(parking_lot::Mutex::new(Vec::new())),
            next_submission_id: AtomicU64::new(1),
            next_question_id: AtomicU64::new(1),
            next_run_seq: AtomicU64::new(1),
            event_seq: AtomicU64::new(1),
            derive_counter: AtomicU64::new(0),
        }
    }

    // -- submission pipeline -------------------------------------------------

    /// Entry point: assesses `prompt`, parks on clarifying questions when
    /// ambiguous, otherwise expands into fresh tree nodes and lets the
    /// scheduler take over. Returns immediately — the pipeline runs in the
    /// background.
    pub fn submit(self: &Arc<Self>, prompt: &str) -> SubmissionHandle {
        let id = self.next_submission_id.fetch_add(1, Ordering::Relaxed);
        let prompt = prompt.to_owned();
        let this = Arc::clone(self);
        let pipeline = tokio::spawn(async move {
            this.run_pipeline(id, prompt).await;
        });
        self.pipelines.lock().push(pipeline);
        self.ensure_scheduler();
        SubmissionHandle {
            id,
            orch: Arc::clone(self),
        }
    }

    /// Delivers a human answer to a pending clarifying question; the parked
    /// pipeline resumes exactly where it left off.
    pub fn answer(&self, answer: Answer) -> Result<(), crate::mailbox::MailboxError> {
        self.mailbox.answer(answer)
    }

    async fn run_pipeline(self: Arc<Self>, submission_id: u64, prompt: String) {
        // Phase 1: ambiguity gate.
        let verdict = match self.planner.assess(&prompt).await {
            Ok(v) => v,
            Err(_) => {
                self.mark_submission(submission_id, Phase::PlannerFailed)
                    .await;
                return;
            }
        };
        let clarified = if verdict.clear {
            prompt
        } else {
            // Phase 2a: park on sequentially-numbered questions.
            let mut receivers = Vec::new();
            for q in verdict.questions {
                let id = self.next_question_id.fetch_add(1, Ordering::Relaxed);
                match self.mailbox.ask(q.into_question(id)) {
                    Ok(rx) => receivers.push(rx),
                    Err(_) => continue, // duplicate id: drop rather than wedge
                }
            }
            self.mark_submission(submission_id, Phase::ParkedAwaitingAnswers)
                .await;
            let mut clarified = format!("{prompt}\n\nClarifications:");
            for rx in receivers {
                let text = match rx.await {
                    Ok(answer) => answer
                        .choice
                        .or(answer.free_text)
                        .unwrap_or_else(|| "(empty answer)".into()),
                    Err(_) => "(retracted)".to_owned(),
                };
                clarified.push_str("\n- ");
                clarified.push_str(&text);
            }
            clarified
        };

        // Phase 3: expand into the shared tree.
        {
            let mut state = self.state.lock().await;
            match self.planner.expand(&mut state.tree, None, &clarified).await {
                Ok(leaves) => {
                    let record = state.submissions.entry(submission_id).or_insert_with(|| {
                        SubmissionRecord {
                            phase: Phase::Assessing,
                            leaves: Vec::new(),
                        }
                    });
                    record.phase = Phase::Expanded;
                    record.leaves = leaves;
                }
                Err(_) => {
                    self.record_event(
                        &mut state,
                        SupervisionEvent {
                            seq: 0,
                            task: TaskId(0),
                            kind: SupervisionEventKind::Failed,
                            policy: Some(RecoveryPolicy::Escalate(
                                "planner expansion failed".into(),
                            )),
                            stall: None,
                            detail: format!("submission {submission_id} expansion failed"),
                        },
                    );
                    self.mark_submission(submission_id, Phase::PlannerFailed)
                        .await;
                    return;
                }
            }
        }
        // Nudge the scheduler so expansion starts without waiting a tick.
        let _ = self.msg_tx.send(Msg::Wake);
    }

    /// Writes a submission lifecycle phase under the state lock.
    async fn mark_submission(&self, id: u64, phase: Phase) {
        let mut state = self.state.lock().await;
        let record = state
            .submissions
            .entry(id)
            .or_insert_with(|| SubmissionRecord {
                phase: Phase::Assessing,
                leaves: Vec::new(),
            });
        record.phase = phase;
    }

    // -- scheduling core -------------------------------------------------------

    /// Spawns THE scheduler task exactly once (idempotent).
    fn ensure_scheduler(self: &Arc<Self>) {
        let mut slot = self.scheduler.lock();
        if slot.is_some() {
            return;
        }
        let Some(mut rx) = self.msg_rx.lock().take() else {
            return;
        };
        let weak = Arc::downgrade(self);
        let ticker_period = self.config.poll_interval;
        let shutdown = self.shutdown.clone();
        let join = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(ticker_period);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                let Some(this) = weak.upgrade() else {
                    break; // orchestrator dropped: stop supervising
                };
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    msg = rx.recv() => match msg {
                        Some(Msg::Done(task, seq)) => this.on_done(task, seq).await,
                        Some(Msg::Wake) => {}
                        None => break,
                    },
                    _ = ticker.tick() => this.maintenance_tick().await,
                }
                this.fill_free_slots().await;
            }
        });
        *slot = Some(join);
    }

    /// Tick work: mid-flight input, watchdog sweep, recovery application.
    async fn maintenance_tick(self: &Arc<Self>) {
        self.drain_inbox().await;
        let actions = {
            let state = self.state.lock().await;
            self.watchdog.sweep(&state.handles)
        };
        if !actions.is_empty() {
            self.apply_recovery(actions).await;
        }
    }

    /// Consumes injected user messages: each triggers a Planner re-expand
    /// appended under the same tree (v1 simplification, see module docs).
    async fn drain_inbox(self: &Arc<Self>) {
        let messages: Vec<String> = std::mem::take(&mut *self.inbox.lock());
        for message in messages {
            let mut state = self.state.lock().await;
            if let Ok(leaves) = self.planner.expand(&mut state.tree, None, &message).await {
                if !leaves.is_empty() {
                    let _ = self.msg_tx.send(Msg::Wake);
                }
            }
        }
    }

    /// Starts ready leaves while free slots remain.
    async fn fill_free_slots(self: &Arc<Self>) {
        loop {
            let prepared = {
                let mut state = self.state.lock().await;
                let free = self
                    .config
                    .max_parallel_leaves
                    .saturating_sub(state.active_runs);
                if free == 0 {
                    None
                } else {
                    self.prepare_launch(&mut state)
                }
            };
            match prepared {
                Some(parts) => parts.spawn(),
                None => break,
            }
        }
    }

    /// Picks one ready leaf and fully books it (status, assignment, handle,
    /// worker acquisition, audit event). Returns the worker-side spawn kit.
    fn prepare_launch(&self, state: &mut OrchestraState) -> Option<SpawnParts> {
        for task in state.tree.next_ready() {
            let Some(node) = state.tree.get(task) else {
                continue;
            };
            if node.attempts >= self.config.max_task_attempts {
                // Safety valve: a Pending task past budget must never start.
                let reason = format!("attempt budget exhausted ({} starts)", node.attempts);
                let _ = state.tree.set_status(task, NodeStatus::Blocked);
                state.escalated.insert(task);
                self.record_event(
                    state,
                    SupervisionEvent {
                        seq: 0,
                        task,
                        kind: SupervisionEventKind::Escalation,
                        policy: Some(RecoveryPolicy::Escalate(reason.clone())),
                        stall: None,
                        detail: reason,
                    },
                );
                continue;
            }
            let preferred = node.title.clone();
            let base_brief = node.brief.clone();
            let hints = state.amendments.get(&task).cloned().unwrap_or_default();

            // Book the run sequence BEFORE composing, so the worker's input
            // carries a per-attempt tag (audit + lets executors key
            // behaviour per supervised attempt).
            let seq = self.next_run_seq.fetch_add(1, Ordering::Relaxed);
            let mut brief = compose_brief(&base_brief, &hints);
            brief.push_str(&format!("\n[supervisor attempt {seq}]"));

            let exclude = state.excluded.get(&task).cloned();
            let Some((entry, agent)) = self.acquire_worker(&preferred, exclude.as_deref()) else {
                return None; // pool unusable this round; retry next tick
            };
            let agent_id = entry.agent_id().to_owned();

            let (handle, guard) = RunHandle::pair(task);
            let _ = state.tree.set_status(task, NodeStatus::InProgress);
            let _ = state.tree.assign(task, Some(agent_id.clone()));
            state.handles.insert(task, handle);
            state.meta.insert(
                task,
                RunMeta {
                    entry,
                    agent_id: agent_id.clone(),
                    seq,
                },
            );
            state.active_runs += 1;
            state.active_peak = state.active_peak.max(state.active_runs);
            state.excluded.remove(&task);
            self.record_event(
                state,
                SupervisionEvent {
                    seq: 0,
                    task,
                    kind: SupervisionEventKind::Delegated,
                    policy: None,
                    stall: None,
                    detail: format!("delegated to {agent_id}"),
                },
            );
            return Some(SpawnParts {
                guard,
                agent,
                brief,
                task,
                seq,
                ledger: self.watchdog.ledger().clone(),
                tx: self.msg_tx.clone(),
            });
        }
        None
    }

    /// Acquires a pooled worker, preferring `preferred` specialty, avoiding
    /// `exclude` when present. Exclusion uses remove→acquire→register over
    /// public registry APIs (documented limitation workaround). Grows the
    /// pool by deriving from the base agent when nothing idle remains —
    /// pool growth is this module's responsibility.
    fn acquire_worker(
        &self,
        preferred: &str,
        exclude: Option<&str>,
    ) -> Option<(Arc<dyn WorkerAdapter>, Arc<Agent>)> {
        let mut removed = None;
        if let Some(excluded_id) = exclude {
            removed = self.registry.remove(excluded_id).ok();
        }
        let acquired = self.registry.acquire(&[preferred]);
        if let Some(entry) = removed {
            let _ = self.registry.register(entry); // restore for future reuse
        }
        if let Some(entry) = acquired {
            // Clone the agent out first: `as_agent` borrows the entry.
            match entry.as_agent().map(Arc::clone) {
                Some(agent) => return Some((entry, agent)),
                None => {
                    // Placeholder entry (no agent): release and fall through.
                    self.registry.release(&entry);
                }
            }
        }
        // Pool exhausted: grow it with a derived specialist.
        for _ in 0..16 {
            let n = self.derive_counter.fetch_add(1, Ordering::Relaxed);
            let suffix = format!("-worker-{n}");
            let entry = derive_entry(&self.base_agent, &suffix, vec![preferred.to_owned()]);
            let id = format!("{}{suffix}", self.base_agent.id());
            match self.registry.register(entry) {
                Ok(()) => {
                    let claimed = self.registry.get(&id)?;
                    if claimed.try_claim() {
                        let agent = claimed.as_agent().map(Arc::clone)?;
                        return Some((claimed, agent));
                    }
                    return None;
                }
                Err(crate::registry::RegistryError::DuplicateId(_)) => continue,
                Err(_) => return None,
            }
        }
        None
    }

    // -- completion ------------------------------------------------------------

    /// Retires a finished run: releases its worker, maps the outcome onto
    /// the tree, propagates completion, retries natural failures within the
    /// attempt budget.
    async fn on_done(self: &Arc<Self>, task: TaskId, seq: u64) {
        let mut state = self.state.lock().await;
        // Stale notification (superseded or already-retired run): ignore.
        if !matches!(state.meta.get(&task), Some(meta) if meta.seq == seq) {
            return;
        }
        let Some(meta) = state.meta.remove(&task) else {
            return;
        };
        let Some(mut handle) = state.handles.remove(&task) else {
            return;
        };
        let agent_id = meta.agent_id.clone();
        let _ = self.registry.release(&meta.entry);
        state.active_runs = state.active_runs.saturating_sub(1);

        let outcome = handle.try_result().unwrap_or(TaskOutcome::Cancelled);
        match outcome {
            TaskOutcome::Success(text) => {
                let _ = state.tree.set_status(task, NodeStatus::Completed);
                let unblocked = state.tree.propagate_completion(task).unwrap_or_default();
                state.excluded.remove(&task);
                self.record_event(
                    &mut state,
                    SupervisionEvent {
                        seq: 0,
                        task,
                        kind: SupervisionEventKind::Completed,
                        policy: None,
                        stall: None,
                        detail: format!("{agent_id}: {}", truncate_for_audit(&text)),
                    },
                );
                let _ = unblocked; // unblocked nodes simply rejoin next_ready
            }
            TaskOutcome::Failed(err) => {
                let attempts = state.tree.get(task).map(|n| n.attempts).unwrap_or(0);
                let _ = state.tree.mark_failed(task, err.clone());
                self.record_event(
                    &mut state,
                    SupervisionEvent {
                        seq: 0,
                        task,
                        kind: SupervisionEventKind::Failed,
                        policy: None,
                        stall: None,
                        detail: format!("{agent_id}: {}", truncate_for_audit(&err)),
                    },
                );
                if attempts < self.config.max_task_attempts {
                    let _ = state.tree.retry(task);
                    self.record_event(
                        &mut state,
                        SupervisionEvent {
                            seq: 0,
                            task,
                            kind: SupervisionEventKind::Recovery,
                            policy: Some(RecoveryPolicy::Retry),
                            stall: None,
                            detail: "natural failure: automatic retry".into(),
                        },
                    );
                } else {
                    let reason = format!("retries exhausted after {attempts} attempts");
                    let _ = state.tree.retry(task); // Failed → Pending…
                    let _ = state.tree.set_status(task, NodeStatus::Blocked); // …→ park
                    state.escalated.insert(task);
                    self.record_event(
                        &mut state,
                        SupervisionEvent {
                            seq: 0,
                            task,
                            kind: SupervisionEventKind::Escalation,
                            policy: Some(RecoveryPolicy::Escalate(reason.clone())),
                            stall: None,
                            detail: reason,
                        },
                    );
                }
            }
            TaskOutcome::Cancelled => {
                let _ = state.tree.set_status(task, NodeStatus::Cancelled);
                self.record_event(
                    &mut state,
                    SupervisionEvent {
                        seq: 0,
                        task,
                        kind: SupervisionEventKind::Cancelled,
                        policy: None,
                        stall: None,
                        detail: format!("run on {agent_id} cancelled"),
                    },
                );
            }
        }
    }

    // -- supervision -----------------------------------------------------------

    /// Applies watchdog recommendations to live runs. Restarting policies
    /// cancel the current run, release its worker, and reschedule the task;
    /// the attempt budget decides between retry and escalation.
    async fn apply_recovery(self: &Arc<Self>, actions: Vec<crate::watchdog::SupervisionAction>) {
        for action in actions {
            let mut state = self.state.lock().await;
            if !state.handles.contains_key(&action.task) {
                continue; // run already retired; stale verdict
            }
            let task = action.task;
            match &action.recommended {
                RecoveryPolicy::Nudge => {
                    self.record_event(
                        &mut state,
                        SupervisionEvent {
                            seq: 0,
                            task,
                            kind: SupervisionEventKind::Recovery,
                            policy: Some(RecoveryPolicy::Nudge),
                            stall: Some(action.kind),
                            detail: "nudged; no state change".into(),
                        },
                    );
                }
                RecoveryPolicy::Retry => {
                    self.restart_run(&mut state, &action, None);
                }
                RecoveryPolicy::RespawnAmended(hint) => {
                    state.amendments.entry(task).or_default().push(hint.clone());
                    self.restart_run(&mut state, &action, Some(hint.clone()));
                }
                RecoveryPolicy::Reassign => {
                    if let Some(agent_id) = state.meta.get(&task).map(|m| m.agent_id.clone()) {
                        state.excluded.insert(task, agent_id);
                    }
                    self.restart_run(&mut state, &action, None);
                }
                RecoveryPolicy::Escalate(reason) => {
                    self.kill_run(&mut state, task);
                    let _ = state.tree.mark_failed(task, reason.clone());
                    let _ = state.tree.retry(task); // Failed → Pending
                    let _ = state.tree.set_status(task, NodeStatus::Blocked); // park
                    state.escalated.insert(task);
                    self.record_event(
                        &mut state,
                        SupervisionEvent {
                            seq: 0,
                            task,
                            kind: SupervisionEventKind::Escalation,
                            policy: Some(action.recommended.clone()),
                            stall: Some(action.kind),
                            detail: reason.clone(),
                        },
                    );
                }
            }
        }
    }

    /// Cancels the current run and reschedules: `mark_failed → retry`. Past
    /// the attempt budget the task escalates to `Blocked` instead.
    fn restart_run(
        &self,
        state: &mut OrchestraState,
        action: &crate::watchdog::SupervisionAction,
        amended_hint: Option<String>,
    ) {
        let task = action.task;
        self.kill_run(state, task);
        let attempts = state.tree.get(task).map(|n| n.attempts).unwrap_or(0);
        let _ = state
            .tree
            .mark_failed(task, format!("watchdog: {:?}", action.kind));
        if attempts >= self.config.max_task_attempts {
            let reason = format!(
                "{} stalled; retries exhausted after {attempts} attempts",
                action.kind
            );
            let _ = state.tree.retry(task);
            let _ = state.tree.set_status(task, NodeStatus::Blocked);
            state.escalated.insert(task);
            self.record_event(
                state,
                SupervisionEvent {
                    seq: 0,
                    task,
                    kind: SupervisionEventKind::Escalation,
                    policy: Some(RecoveryPolicy::Escalate(reason.clone())),
                    stall: Some(action.kind),
                    detail: reason,
                },
            );
            return;
        }
        // Reschedule (the amendment overlay was already recorded by the
        // caller, so the respawned worker composes the amended brief).
        let _ = state.tree.retry(task);
        self.record_event(
            state,
            SupervisionEvent {
                seq: 0,
                task,
                kind: SupervisionEventKind::Recovery,
                policy: Some(action.recommended.clone()),
                stall: Some(action.kind),
                detail: match &amended_hint {
                    Some(hint) => format!("respawned with amendment: {hint}"),
                    None => "restarting".to_owned(),
                },
            },
        );
    }

    /// Stops one live run and frees its worker WITHOUT touching the tree —
    /// callers decide the follow-up transitions.
    fn kill_run(&self, state: &mut OrchestraState, task: TaskId) {
        if let Some(handle) = state.handles.remove(&task) {
            handle.cancel(); // the worker observes this and drops its guard
        }
        if let Some(meta) = state.meta.remove(&task) {
            let _ = self.registry.release(&meta.entry);
            state.active_runs = state.active_runs.saturating_sub(1);
        }
    }

    fn record_event(&self, state: &mut OrchestraState, mut event: SupervisionEvent) {
        event.seq = self.event_seq.fetch_add(1, Ordering::Relaxed);
        state.events.push(event);
    }

    // -- public controls ---------------------------------------------------------

    /// Cancels a running task (or withdraws a pending one). Dependents of a
    /// cancelled task are never scheduled: their dependency can no longer
    /// reach `Completed`.
    pub async fn cancel_task(&self, task: TaskId) -> Result<(), TreeError> {
        let mut state = self.state.lock().await;
        if let Some(handle) = state.handles.get(&task) {
            handle.cancel();
            return Ok(());
        }
        state.tree.set_status(task, NodeStatus::Cancelled)
    }

    /// Queues a mid-flight user message; consumed by the next scheduler
    /// tick, where it triggers a Planner re-expand under the same tree.
    pub fn inject_user_message(&self, text: impl Into<String>) {
        self.inbox.lock().push(text.into());
    }

    /// Operator snapshot: counts, pending answers, occupancy, escalations.
    pub async fn status_report(&self) -> OrchestratorStatus {
        let state = self.state.lock().await;
        OrchestratorStatus {
            counts_by_status: state.tree.counts_by_status(),
            awaiting_answers: self.mailbox.pending_count(),
            active_runs: state.active_runs,
            active_peak: state.active_peak,
            escalated: state.escalated.iter().copied().collect(),
            submissions: state.submissions.len(),
        }
    }

    /// Snapshot of the audit trail (oldest first).
    pub async fn events(&self) -> Vec<SupervisionEvent> {
        self.state.lock().await.events.clone()
    }

    /// The effective brief a (re)spawned worker would receive: node brief
    /// plus any supervisor amendment overlays.
    pub async fn effective_brief(&self, task: TaskId) -> Option<String> {
        let state = self.state.lock().await;
        let node = state.tree.get(task)?;
        let hints = state.amendments.get(&task);
        Some(compose_brief(
            &node.brief,
            hints.map(Vec::as_slice).unwrap_or(&[]),
        ))
    }

    /// Read-only tree access for supervision-side introspection and proofs.
    pub async fn tree_snapshot<F, T>(&self, read: F) -> T
    where
        F: FnOnce(&TaskTree) -> T,
    {
        let state = self.state.lock().await;
        read(&state.tree)
    }

    /// The shared progress feed (tests seed this to exercise loop
    /// detection; workers report through the very same surface).
    #[must_use]
    pub fn ledger(&self) -> &ProgressLedger {
        self.watchdog.ledger()
    }

    /// Graceful stop: cancel every live run, stop the scheduler (waiting up
    /// to `timeout`), abort parked pipelines.
    pub async fn shutdown(&self, timeout: Duration) {
        self.shutdown.cancel();
        let join = self.scheduler.lock().take();
        if let Some(join) = join {
            let _ = tokio::time::timeout(timeout, join).await;
        }
        let pipelines: Vec<_> = std::mem::take(&mut *self.pipelines.lock());
        for pipeline in pipelines {
            pipeline.abort();
        }
        // Best-effort: make sure no worker outlives the supervisor.
        let state = self.state.lock().await;
        for handle in state.handles.values() {
            handle.cancel();
        }
    }
}

/// Bridge for recording non-Expanded submission phases from sync contexts:
/// (implementation note: all phase writes now flow through the async
/// `mark_submission`; this comment documents that no sync-context write
/// path remains.)
///
/// Worker-side spawn kit produced by [`Orchestrator::prepare_launch`].
struct SpawnParts {
    guard: RunGuard,
    agent: Arc<Agent>,
    brief: String,
    task: TaskId,
    seq: u64,
    ledger: ProgressLedger,
    tx: mpsc::UnboundedSender<Msg>,
}

impl SpawnParts {
    /// Runs the delegated work: note start progress, race the agent against
    /// cancellation, report through the guard, notify the loop.
    fn spawn(self) {
        let Self {
            guard,
            agent,
            brief,
            task,
            seq,
            ledger,
            tx,
        } = self;
        tokio::spawn(async move {
            ledger.note_progress(task, &format!("start {brief}"));
            let token = guard.token().clone();
            let run = agent.run(&brief);
            tokio::pin!(run);
            let outcome = tokio::select! {
                result = &mut run => match result {
                    Ok(result) => {
                        ledger.note_progress(task, &result.text);
                        TaskOutcome::Success(result.text)
                    }
                    Err(err) => TaskOutcome::Failed(err.to_string()),
                },
                _ = token.cancelled() => TaskOutcome::Cancelled,
            };
            match outcome {
                TaskOutcome::Cancelled => drop(guard), // guard Drop records it
                other => guard.finish(other),
            }
            let _ = tx.send(Msg::Done(task, seq));
        });
    }
}

/// Audit strings stay bounded.
fn truncate_for_audit(s: &str) -> String {
    const LIMIT: usize = 160;
    if s.chars().count() <= LIMIT {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(LIMIT).collect();
        format!("{cut}…")
    }
}

// -- submission handle ---------------------------------------------------------

/// Caller-side view of one submitted prompt: correlate it and poll its
/// lifecycle without touching fleet internals.
#[derive(Clone)]
pub struct SubmissionHandle {
    id: u64,
    orch: Arc<Orchestrator>,
}

impl SubmissionHandle {
    /// The submission's correlation id.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Current lifecycle stage, derived from shared state.
    pub async fn status(&self) -> SubmissionStatus {
        let state = self.orch.state.lock().await;
        let Some(record) = state.submissions.get(&self.id) else {
            return SubmissionStatus::Assessing;
        };
        match record.phase {
            Phase::Assessing => SubmissionStatus::Assessing,
            Phase::ParkedAwaitingAnswers => SubmissionStatus::AwaitingAnswers,
            Phase::PlannerFailed => SubmissionStatus::Failed,
            Phase::Expanded => {
                let mut all_completed = true;
                let mut any_open = false;
                for leaf in &record.leaves {
                    match state.tree.get(*leaf).map(|n| n.status) {
                        Some(NodeStatus::Completed) => {}
                        Some(
                            NodeStatus::Pending | NodeStatus::InProgress | NodeStatus::Blocked,
                        ) => {
                            any_open = true;
                            all_completed = false;
                        }
                        _ => all_completed = false,
                    }
                }
                if any_open {
                    SubmissionStatus::Running
                } else if all_completed && !record.leaves.is_empty() {
                    SubmissionStatus::Completed
                } else {
                    SubmissionStatus::Failed
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_match_spec() {
        let c = OrchestratorConfig::default();
        assert_eq!(c.max_parallel_leaves, 8);
        assert_eq!(c.poll_interval, Duration::from_millis(50));
        assert_eq!(c.max_task_attempts, 3);
        assert_eq!(c.watchdog.progress_window, Duration::from_secs(30));
    }

    #[test]
    fn compose_brief_appends_amendments_in_order() {
        let brief = compose_brief("do the thing", &["first hint".into(), "second".into()]);
        assert!(brief.starts_with("do the thing"));
        assert!(brief.contains("[supervisor amendment] first hint"));
        assert!(brief.contains("[supervisor amendment] second"));
        assert_eq!(compose_brief("solo", &[]), "solo");
    }

    #[test]
    fn truncate_keeps_audit_lines_short() {
        assert_eq!(truncate_for_audit("short"), "short");
        let long = "x".repeat(500);
        assert_eq!(truncate_for_audit(&long).chars().count(), 161);
    }
}
