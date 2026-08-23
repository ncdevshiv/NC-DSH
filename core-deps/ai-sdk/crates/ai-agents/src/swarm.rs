//! Swarm orchestration (PRD §3.10): a template-stamped engine executing
//! many tasks with isolated agents, bounded concurrency, partial-failure
//! semantics, hierarchical map-reduce, competitive elimination, and token
//! budgets.
//!
//! # Architecture (HERCULES)
//!
//! [`SwarmEngine`] replaces the old single-`Arc<Agent>` swarm. That design
//! cloned ONE agent for every concurrent input; because memory is keyed by
//! the agent id, all conversations interleaved under concurrency, and the
//! first task failure aborted the whole run (`failed` was structurally
//! always 0). Both defects are gone:
//!
//! - **Isolation**: the engine holds a [`SwarmTemplate`], a factory called
//!   once per task. Each call produces an agent via [`Agent::derive`] —
//!   same model/tools/observability/HITL, own id, fresh memory — so no two
//!   tasks share conversation state even when they run in parallel.
//! - **Partial failure**: per-task outcomes are collected independently.
//!   [`SwarmResult::results`] is indexed by input (`None` = that task
//!   failed) and [`SwarmResult::failed`] lists `(index, error)` pairs.
//!
//! Three strategies ship as typed entry points:
//! [`SwarmEngine::fan_out`], [`SwarmEngine::map_reduce`], and
//! [`SwarmEngine::competitive`]. Token budgets
//! ([`SwarmEngine::with_task_budget`],
//! [`SwarmEngine::with_swarm_budget`]) abort not-yet-started tasks with a
//! budget-exhausted error (recognizable via [`is_budget_exhausted`]) once
//! the ledger reaches its cap; already in-flight tasks finish, so overshoot
//! is bounded by `concurrency × max single-task cost`.
//!
//! The pre-engine [`AgentSwarm`] survives only as a deprecated thin wrapper
//! so existing re-exports keep compiling; new code should use
//! [`SwarmEngine`] directly.

use std::cmp::Ordering;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use async_trait::async_trait;

use ai_errors::{AgentError, AiError, ValidationError};
use ai_runtime::{Parallel, Task};
use ai_types::Usage;

use crate::agent::{Agent, AgentResult, accumulate_usage};

/// Factory producing one isolated, same-configured [`Agent`] per task slot.
///
/// The argument is the task's position in the fan-out (or the candidate
/// slot for competitive runs). Implementations typically call
/// [`Agent::derive`] on a configured prototype, which shares the model
/// `Arc`, tools, collector/exporters, and HITL hook while minting FRESH
/// memory and an id unique to the slot.
pub type SwarmTemplate = Arc<dyn Fn(usize) -> Agent + Send + Sync>;

/// Marker embedded in every budget-exhausted error message. Budget errors
/// wrap into [`AiError::Agent`] because `ai-errors` has no dedicated
/// variant (that crate is off-limits for this change); recognize them with
/// [`is_budget_exhausted`] instead of string-matching yourself.
pub const BUDGET_EXHAUSTED_MARKER: &str = "token budget exhausted";

/// Builds a budget-exhausted [`AiError`] for the given scope.
fn budget_error(scope: &str, detail: impl std::fmt::Display) -> AiError {
    AiError::Agent(AgentError::new(
        format!("swarm-budget:{scope}"),
        format!("{BUDGET_EXHAUSTED_MARKER}: {detail}"),
    ))
}

/// Whether `err` reports an exhausted token budget.
pub fn is_budget_exhausted(err: &AiError) -> bool {
    err.to_string().contains(BUDGET_EXHAUSTED_MARKER)
}

/// Token ledger shared across one swarm run. Units are tokens as reported
/// by [`Usage::total`].
#[derive(Debug, Default)]
struct BudgetLedger {
    spent: AtomicU64,
}

impl BudgetLedger {
    fn spent(&self) -> u64 {
        self.spent.load(AtomicOrdering::Relaxed)
    }

    fn charge(&self, tokens: u64) {
        self.spent.fetch_add(tokens, AtomicOrdering::Relaxed);
    }
}

/// Outcome of a fan-out run (and of a map-reduce map phase): per-input
/// results in input order with honest partial-failure accounting.
#[derive(Debug, Clone)]
pub struct SwarmResult {
    /// `results[i]` is the outcome of input `i`; `None` marks a failed or
    /// budget-aborted task (see [`SwarmResult::failed`]).
    pub results: Vec<Option<AgentResult>>,
    /// Number of successful tasks.
    pub succeeded: usize,
    /// `(input_index, error message)` for every failed task, ascending by
    /// index.
    pub failed: Vec<(usize, String)>,
    /// Total token usage across all completed model calls of the run.
    pub total_usage: Usage,
}

impl SwarmResult {
    /// The result of input `index`, if that task succeeded.
    pub fn get(&self, index: usize) -> Option<&AgentResult> {
        self.results.get(index).and_then(|r| r.as_ref())
    }

    /// Whether every input succeeded.
    pub fn all_succeeded(&self) -> bool {
        self.failed.is_empty() && self.succeeded == self.results.len()
    }

    /// Whether any task was aborted by a token budget.
    pub fn budget_exhausted(&self) -> bool {
        self.failed
            .iter()
            .any(|(_, e)| e.contains(BUDGET_EXHAUSTED_MARKER))
    }
}

/// One node of the hierarchical reduce tree.
#[derive(Debug, Clone)]
pub struct ReduceNode {
    /// Indices of this node's inputs in the previous level (level-0 nodes
    /// index the MAP outputs).
    pub children: Vec<usize>,
    /// The reduce agent's consolidated text, if the node succeeded.
    pub output: Option<String>,
    /// Why the node failed (all children unavailable, budget abort, …).
    pub error: Option<String>,
    /// Tokens consumed by this node's reduce call.
    pub usage: Usage,
}

/// Outcome of [`SwarmEngine::map_reduce`].
#[derive(Debug, Clone)]
pub struct ReduceOutcome {
    /// The final consolidated answer, if the root node succeeded.
    pub root: Option<String>,
    /// Map-phase detail (per-input results and failures).
    pub map: SwarmResult,
    /// Reduce tree levels; `levels[0]` combines the map outputs and each
    /// following level combines the previous one.
    pub levels: Vec<Vec<ReduceNode>>,
    /// Total tokens across the map AND all reduce phases.
    pub total_usage: Usage,
}

/// Pairwise judge for the competitive strategy.
#[async_trait]
pub trait JudgeFn: Send + Sync {
    /// Ranks two answers for `task`: [`Ordering::Greater`] means
    /// `answer_a` outranks `answer_b`.
    async fn judge(&self, task: &str, answer_a: &str, answer_b: &str) -> Result<Ordering, AiError>;
}

/// One competitive round's record.
#[derive(Debug, Clone)]
pub struct RoundRecord {
    /// Round number, 1-based.
    pub round: usize,
    /// `(candidate_slot, answer)` for every candidate that produced an
    /// answer this round, ordered best first.
    pub answers: Vec<(usize, String)>,
    /// Candidate slots eliminated at the end of this round: the bottom
    /// ranked fraction, plus any candidate whose agent run failed.
    pub eliminated: Vec<usize>,
}

/// Final standing of one candidate slot.
#[derive(Debug, Clone)]
pub struct CompetitiveScore {
    /// Candidate slot (the index passed to the template).
    pub candidate: usize,
    /// Rounds in which this candidate produced an answer.
    pub survived_rounds: u32,
    /// 0 = winner. Eliminated candidates rank after survivors, most
    /// recently eliminated first.
    pub final_rank: usize,
}

/// Outcome of [`SwarmEngine::competitive`].
#[derive(Debug, Clone)]
pub struct CompetitiveOutcome {
    /// Winning candidate slot.
    pub winner: Option<usize>,
    /// The winning answer text.
    pub winner_answer: Option<String>,
    /// Final ranking, winner first.
    pub scores: Vec<CompetitiveScore>,
    /// Per-round ledger (answers + eliminations).
    pub rounds: Vec<RoundRecord>,
    /// Total tokens across all candidate runs.
    pub total_usage: Usage,
}

/// Template-driven swarm executor with bounded concurrency, partial-failure
/// collection, three strategies, and token budgets.
///
/// Construct with [`SwarmEngine::new`]; configuration methods chain. The
/// engine holds one template `Arc` plus scalars, so cloning is cheap, and
/// it is `Send + Sync`.
#[derive(Clone)]
pub struct SwarmEngine {
    template: SwarmTemplate,
    reduce_template: Option<SwarmTemplate>,
    /// Maximum concurrent agent executions.
    concurrency: usize,
    /// Maximum children combined per reduce node (>= 2).
    max_fan_in: usize,
    /// Per-task token cap ([`Usage::total`]); a completed task exceeding it
    /// fails with a budget error.
    task_token_budget: Option<u64>,
    /// Whole-swarm token cap shared by ALL strategies and phases.
    swarm_token_budget: Option<u64>,
    /// Fraction of bottom-ranked candidates eliminated per competitive
    /// round, clamped to `[0.0, 1.0)`; `0.0` ranks without eliminating.
    eliminate_fraction: f32,
}

impl std::fmt::Debug for SwarmEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwarmEngine")
            .field("concurrency", &self.concurrency)
            .field("max_fan_in", &self.max_fan_in)
            .field("task_token_budget", &self.task_token_budget)
            .field("swarm_token_budget", &self.swarm_token_budget)
            .field("eliminate_fraction", &self.eliminate_fraction)
            .field("has_reduce_template", &self.reduce_template.is_some())
            .finish()
    }
}

/// One completed reduce node's payload: its children (indices into the
/// previous level), consolidated output text if any, and token usage.
type ReduceTaskOutput = (Vec<usize>, Option<String>, Usage);

impl SwarmEngine {
    /// Creates an engine stamping agents from `template`. Defaults:
    /// concurrency 4, `max_fan_in` 4, no budgets, elimination fraction 0.5.
    pub fn new(template: SwarmTemplate) -> Self {
        Self {
            template,
            reduce_template: None,
            concurrency: 4,
            max_fan_in: 4,
            task_token_budget: None,
            swarm_token_budget: None,
            eliminate_fraction: 0.5,
        }
    }

    /// Maximum concurrent agent executions across every strategy.
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency.max(1);
        self
    }

    /// Template minting the REDUCE agents of [`map_reduce`](Self::map_reduce).
    pub fn with_reduce_template(mut self, template: SwarmTemplate) -> Self {
        self.reduce_template = Some(template);
        self
    }

    /// Maximum children combined per hierarchical reduce node (minimum 2).
    pub fn with_max_fan_in(mut self, max_fan_in: usize) -> Self {
        self.max_fan_in = max_fan_in.max(2);
        self
    }

    /// Per-task token cap. A completed task whose usage exceeds the cap is
    /// converted into a budget failure. Enforced post-run (the agent API has
    /// no mid-run checkpoint); the swarm-wide budget provides the hard stop
    /// against runaway spend.
    pub fn with_task_budget(mut self, max_tokens_per_task: u64) -> Self {
        self.task_token_budget = Some(max_tokens_per_task.max(1));
        self
    }

    /// Whole-swarm token cap across all tasks/phases of one run. Once the
    /// ledger reaches the cap, not-yet-started tasks fail immediately with
    /// a budget error instead of running ("abort remaining"); in-flight
    /// tasks finish, bounding overshoot at
    /// `concurrency × max single-task usage`.
    pub fn with_swarm_budget(mut self, max_tokens_total: u64) -> Self {
        self.swarm_token_budget = Some(max_tokens_total.max(1));
        self
    }

    /// Fraction of bottom-ranked candidates eliminated per competitive round
    /// (clamped to `[0.0, 1.0)`). `0.0` ranks without eliminating; the
    /// winner is then the top-ranked agent after the last round.
    pub fn with_eliminate_fraction(mut self, fraction: f32) -> Self {
        self.eliminate_fraction = fraction.clamp(0.0, 0.999_999);
        self
    }

    // -- Strategy a) FanOut --------------------------------------------------

    /// Runs every input through its OWN derived agent with bounded
    /// concurrency, collecting partial failures instead of aborting.
    pub async fn fan_out(&self, inputs: Vec<String>) -> Result<SwarmResult, AiError> {
        let ledger = Arc::new(BudgetLedger::default());
        Ok(self.run_map(&ledger, inputs).await)
    }

    /// Shared fan-out/map phase against an EXISTING ledger so a swarm
    /// budget spans map AND reduce phases.
    async fn run_map(&self, ledger: &Arc<BudgetLedger>, inputs: Vec<String>) -> SwarmResult {
        let (results, failed, usage) = self.execute(ledger, &inputs).await;
        SwarmResult {
            succeeded: results.iter().filter(|r| r.is_some()).count(),
            results,
            failed,
            total_usage: usage,
        }
    }

    // -- Strategy b) MapReduce -----------------------------------------------

    /// Map-reduce: the map phase is [`fan_out`](Self::fan_out); the reduce
    /// phase hierarchically combines chunks of the previous level with
    /// agents stamped from the separate reduce template until a single root
    /// remains.
    ///
    /// Map outputs missing because their task failed contribute an explicit
    /// `[unavailable]` marker; a node whose children are ALL unavailable
    /// fails WITHOUT a model call, and failure propagates upward.
    pub async fn map_reduce(&self, inputs: Vec<String>) -> Result<ReduceOutcome, AiError> {
        let reduce_template = self.reduce_template.clone().ok_or_else(|| {
            AiError::Validation(ValidationError::new(
                "map_reduce requires a reduce template (with_reduce_template)",
            ))
        })?;

        let ledger = Arc::new(BudgetLedger::default());
        let map = self.run_map(&ledger, inputs).await;
        let mut total_usage = map.total_usage;

        // Level values start as the map outputs.
        let mut level: Vec<Option<String>> = map
            .results
            .iter()
            .map(|r| r.as_ref().map(|r| r.text.clone()))
            .collect();

        let mut levels: Vec<Vec<ReduceNode>> = Vec::new();
        let mut node_counter = 0usize;
        let fan_in = self.max_fan_in;

        while level.len() > 1 {
            // Chunk THIS level's indices into reduce groups.
            let chunks: Vec<Vec<usize>> = (0..level.len())
                .collect::<Vec<_>>()
                .chunks(fan_in)
                .map(<[usize]>::to_vec)
                .collect();

            let tasks: Vec<Task<ReduceTaskOutput>> = chunks
                .into_iter()
                .map(|children| {
                    let level_snapshot = level.clone();
                    let template = reduce_template.clone();
                    let ledger = ledger.clone();
                    let swarm_cap = self.swarm_token_budget;
                    let node_index = node_counter;
                    Task::new(format!("reduce:{node_index}"), async move {
                        // Budget preflight: skip the model call once the
                        // swarm allowance is gone.
                        if let Some(cap) = swarm_cap {
                            if ledger.spent() >= cap {
                                return Err(budget_error(
                                    "reduce",
                                    format!("reduce node {node_index} skipped"),
                                ));
                            }
                        }

                        if level_snapshot.iter().all(Option::is_none) {
                            return Ok((children, None, Usage::default()));
                        }

                        let parts: Vec<String> = children
                            .iter()
                            .map(|child| match &level_snapshot[*child] {
                                Some(text) => format!("- {text}"),
                                None => "- [unavailable]".to_string(),
                            })
                            .collect();
                        let prompt = format!(
                            "Combine these partial results into one \
                             consolidated result.\n{}",
                            parts.join("\n")
                        );

                        let agent = template(node_index);
                        match agent.run(&prompt).await {
                            Ok(result) => {
                                // Charge before returning so later nodes
                                // preflight against real spend.
                                ledger.charge(result.usage.total());
                                Ok((children, Some(result.text), result.usage))
                            }
                            Err(e) => Err(e),
                        }
                    })
                })
                .collect();

            let executed = Parallel::new()
                .with_limit(self.concurrency)
                .execute(tasks)
                .await;

            let mut next_level: Vec<Option<String>> = Vec::with_capacity(executed.len());
            let mut nodes: Vec<ReduceNode> = Vec::with_capacity(executed.len());
            for chunk_result in executed {
                match chunk_result.outcome {
                    Ok((children, output, usage)) => {
                        // Ledger already charged inside the task.
                        accumulate_usage(&mut total_usage, usage);
                        next_level.push(output.clone());
                        nodes.push(ReduceNode {
                            children,
                            output,
                            error: None,
                            usage,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("reduce node failed: {e}");
                        next_level.push(None);
                        nodes.push(ReduceNode {
                            children: Vec::new(),
                            output: None,
                            error: Some(e.to_string()),
                            usage: Usage::default(),
                        });
                    }
                }
            }
            levels.push(nodes);
            level = next_level;
            node_counter += 1;
        }

        Ok(ReduceOutcome {
            root: level.into_iter().flatten().next(),
            map,
            levels,
            total_usage,
        })
    }

    // -- Strategy c) Competitive ---------------------------------------------

    /// Competitive rounds: every surviving candidate answers `task`, the
    /// judge ranks them pairwise, and the bottom
    /// [`eliminate_fraction`](Self::with_eliminate_fraction) is removed
    /// each round. After `rounds` rounds (or once one candidate remains)
    /// the top-ranked survivor wins.
    ///
    /// A candidate whose agent run fails is eliminated immediately for that
    /// round; if EVERY candidate fails, the tournament ends early with
    /// whatever the ledger recorded.
    pub async fn competitive(
        &self,
        task: &str,
        candidates: usize,
        rounds: usize,
        judge: Arc<dyn JudgeFn>,
    ) -> Result<CompetitiveOutcome, AiError> {
        if candidates < 2 {
            return Err(AiError::Validation(ValidationError::new(
                "competitive requires at least 2 candidates",
            )));
        }
        if rounds == 0 {
            return Err(AiError::Validation(ValidationError::new(
                "competitive requires at least 1 round",
            )));
        }

        let ledger = Arc::new(BudgetLedger::default());
        let mut total_usage = Usage::default();
        let mut survivors: Vec<usize> = (0..candidates).collect();
        let mut round_records: Vec<RoundRecord> = Vec::new();
        let mut survived_rounds = vec![0u32; candidates];

        for round in 1..=rounds {
            if survivors.len() <= 1 {
                break;
            }

            // Every surviving candidate answers concurrently.
            let tasks: Vec<Task<(usize, String, Usage)>> = survivors
                .iter()
                .copied()
                .map(|slot| {
                    let agent = (self.template)(slot);
                    let task = task.to_string();
                    let ledger = ledger.clone();
                    let swarm_cap = self.swarm_token_budget;
                    Task::new(format!("candidate:{slot}"), async move {
                        if let Some(cap) = swarm_cap {
                            if ledger.spent() >= cap {
                                return Err(budget_error(
                                    "competitive",
                                    format!("candidate {slot} skipped"),
                                ));
                            }
                        }
                        let result = agent.run(&task).await?;
                        // Charge before returning so later candidates
                        // preflight against real spend.
                        ledger.charge(result.usage.total());
                        Ok((slot, result.text, result.usage))
                    })
                })
                .collect();
            let executed = Parallel::new()
                .with_limit(self.concurrency)
                .execute(tasks)
                .await;

            let mut answered: Vec<(usize, String)> = Vec::new();
            let mut failed_slots: Vec<usize> = Vec::new();
            for outcome in executed {
                let slot = outcome
                    .name
                    .strip_prefix("candidate:")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or_default();
                match outcome.outcome {
                    Ok((slot, text, usage)) => {
                        // Ledger already charged inside the task.
                        accumulate_usage(&mut total_usage, usage);
                        survived_rounds[slot] += 1;
                        answered.push((slot, text));
                    }
                    Err(e) => {
                        if !is_budget_exhausted(&e) {
                            tracing::warn!("candidate {slot} failed: {e}");
                        }
                        failed_slots.push(slot);
                    }
                }
            }

            if answered.is_empty() {
                // Whole field failed this round; nothing left to rank.
                round_records.push(RoundRecord {
                    round,
                    answers: Vec::new(),
                    eliminated: failed_slots,
                });
                break;
            }

            // Rank the answered candidates pairwise via insertion sort
            // (best first). n is the survivor count, so O(n²) judge calls
            // are fine.
            for i in 1..answered.len() {
                let mut j = i;
                while j > 0 {
                    let (a_slot, a_answer) = &answered[j - 1];
                    let (_, b_answer) = &answered[j];
                    let ord = judge.judge(task, a_answer, b_answer).await?;
                    if ord == Ordering::Less {
                        tracing::trace!("judge prefers candidate {a_slot} over …");
                        answered.swap(j - 1, j);
                        j -= 1;
                    } else {
                        break;
                    }
                }
            }

            // Eliminate the bottom fraction — never the whole field, so a
            // nonzero fraction still leaves the ranking intact.
            let field = answered.len();
            let k = ((field as f32 * self.eliminate_fraction).floor() as usize)
                .min(field.saturating_sub(1));
            let eliminated_this_round: Vec<usize> = answered[field - k..]
                .iter()
                .map(|(c, _)| *c)
                .chain(failed_slots.iter().copied())
                .collect();

            round_records.push(RoundRecord {
                round,
                answers: answered.clone(),
                eliminated: eliminated_this_round,
            });

            survivors = answered[..field - k].iter().map(|(c, _)| *c).collect();
        }

        // Final ranking: surviving order (ranked last round), then
        // eliminated candidates in reverse elimination order.
        let mut scores: Vec<CompetitiveScore> = Vec::with_capacity(candidates);
        for (rank, candidate) in survivors.iter().enumerate() {
            scores.push(CompetitiveScore {
                candidate: *candidate,
                survived_rounds: survived_rounds[*candidate],
                final_rank: rank,
            });
        }
        for record in round_records.iter().rev() {
            for candidate in &record.eliminated {
                if !scores.iter().any(|s| s.candidate == *candidate) {
                    scores.push(CompetitiveScore {
                        candidate: *candidate,
                        survived_rounds: survived_rounds[*candidate],
                        final_rank: scores.len(),
                    });
                }
            }
        }

        let (winner, winner_answer) = round_records
            .last()
            .and_then(|last| last.answers.first())
            .map(|(c, a)| (Some(*c), Some(a.clone())))
            .unwrap_or((None, None));

        Ok(CompetitiveOutcome {
            winner,
            winner_answer,
            scores,
            rounds: round_records,
            total_usage,
        })
    }

    // -- shared execution core -------------------------------------------------

    /// Executes `inputs` through per-task derived agents, returning results
    /// indexed by position plus `(index, error)` pairs for failures and the
    /// cumulative usage of every completed model call (including tasks that
    /// later fail their per-task budget cap). Each task CHARGES the ledger
    /// as soon as it finishes, so subsequently started tasks see real spend
    /// in their preflight check.
    async fn execute(
        &self,
        ledger: &Arc<BudgetLedger>,
        inputs: &[String],
    ) -> (Vec<Option<AgentResult>>, Vec<(usize, String)>, Usage) {
        let swarm_cap = self.swarm_token_budget;
        let task_cap = self.task_token_budget;
        let usage_total: Arc<std::sync::Mutex<Usage>> =
            Arc::new(std::sync::Mutex::new(Usage::default()));

        let tasks: Vec<Task<AgentResult>> = inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                let input = input.clone();
                let template = self.template.clone();
                let ledger = ledger.clone();
                let usage_total = usage_total.clone();
                Task::new(format!("swarm:{index}"), async move {
                    // Preflight: once the swarm budget is gone, remaining
                    // tasks abort immediately without touching the model.
                    if let Some(cap) = swarm_cap {
                        if ledger.spent() >= cap {
                            return Err(budget_error(
                                "fan-out",
                                format!("task {index} aborted before start"),
                            ));
                        }
                    }
                    let agent = template(index);
                    let result = agent.run(&input).await?;
                    // Bookkeeping happens BEFORE any cap decision: real
                    // spend always charges the ledger and the run total.
                    {
                        let mut acc = usage_total.lock().expect("usage lock");
                        accumulate_usage(&mut acc, result.usage);
                    }
                    ledger.charge(result.usage.total());
                    // Per-task cap: enforced on completion (documented as
                    // post-hoc); the swarm-wide budget is the hard stop.
                    if let Some(cap) = task_cap {
                        if result.usage.total() > cap {
                            return Err(budget_error(
                                "task",
                                format!(
                                    "task {index} used {} tokens > per-task cap {cap}",
                                    result.usage.total()
                                ),
                            ));
                        }
                    }
                    Ok(result)
                })
            })
            .collect();

        let executed = Parallel::new()
            .with_limit(self.concurrency)
            .execute(tasks)
            .await;

        let mut results: Vec<Option<AgentResult>> = vec![None; inputs.len()];
        let mut failed: Vec<(usize, String)> = Vec::new();
        for outcome in executed {
            let index = outcome
                .name
                .strip_prefix("swarm:")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or_default();
            match outcome.outcome {
                Ok(result) => results[index] = Some(result),
                Err(e) => failed.push((index, e.to_string())),
            }
        }
        failed.sort_by_key(|(index, _)| *index);
        let usage = *usage_total.lock().expect("usage lock");
        (results, failed, usage)
    }
}

// ---------------------------------------------------------------------------
// Deprecated legacy wrapper
// ---------------------------------------------------------------------------

/// A bounded swarm sharing ONE configured agent across tasks.
///
/// **Deprecated**: the legacy implementation cloned a single `Arc<Agent>`
/// for every concurrent input, so all conversations interleaved inside one
/// memory scope keyed by the same id, and any single task failure aborted
/// the entire run. This wrapper delegates to [`SwarmEngine`]: each task now
/// derives an isolated agent from the wrapped instance (fresh memory, own
/// id) and failures are collected per-task in [`SwarmResult::failed`]
/// instead of aborting. Prefer [`SwarmEngine`] with an explicit template.
#[deprecated(
    since = "0.1.0",
    note = "legacy shared-agent fan-out; use SwarmEngine with a SwarmTemplate for per-task isolation"
)]
pub struct AgentSwarm {
    engine: SwarmEngine,
}

#[allow(deprecated)]
impl AgentSwarm {
    /// Wraps `agent` as the prototype: every task derives an isolated copy.
    pub fn new(agent: Arc<Agent>) -> Self {
        Self {
            engine: SwarmEngine::new(Arc::new(move |index| agent.derive(&format!("-{index}")))),
        }
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.engine.concurrency = concurrency.max(1);
        self
    }

    /// Runs the swarm over every input with bounded concurrency and
    /// partial-failure semantics (see [`SwarmEngine::fan_out`]).
    pub async fn run(&self, inputs: Vec<String>) -> Result<SwarmResult, AiError> {
        self.engine.fan_out(inputs).await
    }
}
