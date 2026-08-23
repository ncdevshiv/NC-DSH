//! # ai-orchestra — the mechanical core of an orchestrator harness
//!
//! This crate is the offline-testable foundation for a supervisor that turns
//! one prompt into a supervised fleet of sub-agents. It performs ZERO LLM
//! calls: every type and primitive here is deterministic and unit-testable,
//! so later waves can layer model-driven behaviour on trusted machinery.
//!
//! # The orchestrator vision (decompose → clarify → delegate → supervise → adjust)
//!
//! 1. **Decompose** ([`tree`]): an incoming objective becomes a
//!    [`TaskTree`](tree::TaskTree) — nodes carry self-contained briefs (a
//!    fresh agent could execute them with no other context), explicit
//!    dependencies, and a legally-transitioning [`NodeStatus`](tree::NodeStatus).
//! 2. **Clarify** ([`mailbox`] today, `clarifier` in wave B): before work
//!    starts, open questions surface to the human through a
//!    [`QuestionMailbox`](mailbox::QuestionMailbox); answers resolve parked
//!    askers without polling.
//! 3. **Delegate** ([`registry`] today, `expander` in wave B): task leaves go
//!    to pooled workers via the [`WorkerAdapter`](registry::WorkerAdapter)
//!    trait; the [`AgentRegistry`](registry::AgentRegistry) reuses idle
//!    agents — preferring specialty matches — and derives fresh per-task
//!    agents from a base when configured to fan out.
//! 4. **Supervise** ([`handle`] today, `watchdog` in wave B): each delegated
//!    run is a [`RunHandle`](handle::RunHandle) pair — a cancellation token
//!    the supervisor pulls, and an outcome channel the worker pushes. A
//!    dropped guard means the run was never finished, so the outcome is
//!    recorded as cancelled by construction.
//! 5. **Adjust** (`orchestra` in wave B): completion propagates through the
//!    tree (`propagate_completion` unblocks dependents), failures feed retry
//!    transitions, and the ready set (`next_ready`) tells the loop what can
//!    start right now.
//!
//! # Module map
//!
//! | Module       | Wave   | Contents                                        |
//! |--------------|--------|-------------------------------------------------|
//! | [`tree`]     | A      | `TaskTree`, ids, statuses, dependency mechanics |
//! | [`handle`]   | A      | `RunHandle` / `RunGuard`, cancellation plumbing |
//! | [`registry`] | A      | worker pool with reuse + specialization         |
//! | [`mailbox`]  | A      | clarification question/answer mailbox           |
//! | `clarifier`  | B      | prompt → clarifying questions                   |
//! | `expander`   | B      | prompt/task tree decomposition                  |
//! | `watchdog`   | B      | stall detection, budgets, supervision sweeps    |
//! | `orchestra`  | B      | the top-level control loop tying it together    |

#![forbid(unsafe_code)]

pub mod clarifier;
pub mod expander;
pub mod handle;
pub mod mailbox;
pub mod orchestra;
pub mod planner;
pub mod registry;
pub mod tree;
pub mod watchdog;

pub use handle::{RunGuard, RunHandle, TaskOutcome};
pub use mailbox::{Answer, Question, QuestionMailbox};
pub use planner::{ClarifyVerdict, PendingQuestion, Planner};
pub use registry::{AgentEntry, AgentRegistry, WorkerAdapter};
pub use tree::{NodeStatus, TaskId, TaskNode, TaskTree, TreeError};
