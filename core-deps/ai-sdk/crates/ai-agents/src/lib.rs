//! Agent runtime (PRD §3.3, spec §9): lifecycle states, tool loops,
//! sub-agents, HITL hooks, event emission, self-healing retries, and the
//! HERCULES swarm engine.
//!
//! # Agents
//!
//! The loop is explicit and observable: build messages → generate → execute
//! tool calls → repeat until no tool calls or the iteration cap. Every run
//! emits its events into ONE correlated trace (a single trace id with spans
//! nested under the run's root span) when a collector is attached and can
//! persist them durably via [`AgentBuilder::with_exporter`] — the buffer is
//! cleared only after every sink accepted the batch.
//!
//! Memory isolation: by default each [`Agent::run`] executes against an
//! ephemeral run-scoped conversation that is cleared on exit; derived
//! agents ([`Agent::derive`]) mint fresh memory from a [`MemoryFactory`]
//! so swarms never interleave histories. Opt into cross-run persistence
//! with [`AgentBuilder::with_persistent_memory`].
//!
//! Robustness: token usage accumulates across loop iterations; generation
//! retries replay the full request context (system + memory + history);
//! unknown tool names are fed back to the model as error tool-results
//! instead of aborting the run; and a HITL hook may escalate with
//! [`HitlDecision::Escalate`], suspending the run in
//! [`AgentState::AwaitingInput`].
//!
//! # Swarms (HERCULES)
//!
//! [`SwarmEngine`] stamps one isolated agent per task from a
//! [`SwarmTemplate`] and executes strategies with bounded concurrency:
//! [`fan_out`](swarm::SwarmEngine::fan_out) with partial-failure
//! collection, hierarchical [`map_reduce`](swarm::SwarmEngine::map_reduce),
//! and [`competitive`](swarm::SwarmEngine::competitive) elimination judged
//! by a [`JudgeFn`], all under shared token budgets via
//! [`with_swarm_budget`](swarm::SwarmEngine::with_swarm_budget).

mod agent;
mod swarm;

pub use agent::{
    Agent, AgentBuilder, AgentResult, AgentState, AutoApprove, HitlDecision, HumanInTheLoop,
    MemoryFactory, SubAgentHandle, SubAgentTool, default_memory_factory,
};

#[allow(deprecated)]
pub use swarm::{
    AgentSwarm, BUDGET_EXHAUSTED_MARKER, CompetitiveOutcome, CompetitiveScore, JudgeFn, ReduceNode,
    ReduceOutcome, RoundRecord, SwarmEngine, SwarmResult, SwarmTemplate, is_budget_exhausted,
};

#[cfg(test)]
mod tests {
    // Unit tests use deterministic in-memory models (per ADR-007: unit
    // tests mock the LLM; live tests use the real gateway).
}
