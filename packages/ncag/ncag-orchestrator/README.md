---
description: "The NC-AG orchestrator: planning, DAG scheduling, agent lifecycle, resource leases, Git integration, review, verification, decisions, cost and recovery."
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-orchestrator

English | [中文](README.zh.md)

## Summary

This package is the supervisory intelligence. `NcagOrchestrator` owns one platform store, the profile registry and agent instances, the leases, the message log, the decision record, the cost ledger, the model router and the recovery layer. It plans a request, dispatches ready tasks, runs each agent, reviews and verifies the result, integrates an approved change, and records every step on the event stream and in the audit chain.

Each module is independently usable: `planDeterministically`, the DAG helpers, `reviewTask`, `verifyTask`, `LeaseManager`, the Git helpers, `DecisionCenter`, `CostManager`, `withRetry` and `CircuitBreaker` take explicit inputs and keep no hidden global state.

## Table of Contents

- [The orchestrator](#the-orchestrator)
- [Planning](#planning)
- [Scheduling](#scheduling)
- [Execution](#execution)
- [Review and verification](#review-and-verification)
- [Resources and Git](#resources-and-git)
- [Decisions, cost and recovery](#decisions-cost-and-recovery)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="the-orchestrator"></a>
## The orchestrator

| Method | Operation |
|---|---|
| `createProject(input)` | Create a project, ensuring its repository has a base commit; record it |
| `submitRequest({ projectId, rawText })` | Analyze, normalize, decompose, validate the graph, dispatch ready work |
| `dispatch(project)` | Schedule ready tasks under the concurrency and budget ceilings |
| `snapshot()` | The operational snapshot the control plane renders |
| `pauseProject` / `resumeProject` | Stop new dispatch / dispatch the work already held |
| `cancelTask` | Abort the agent, release its leases, record the cancellation |
| `cancelAgent` | Cancel one live agent, release its leases and return its task to the queue |
| `reassignTask` | Re-queue a task under another capability, adopting that profile's permissions |
| `resolveDecision` | Record a decision outcome, apply its consequences, unblock dependants whether accepted or overridden |
| `instructAgent` | Deliver an operator instruction through the durable message log |
| `observe()` | Health pass: detect and recover stalled agents |
| `recoverAfterRestart()` | Restore execution state without duplicating finished work |
| `checkpoint(reason)` | Persist execution state over every project |
| `start()` / `stop()` | Open the platform, resolve routes, start observation / checkpoint and close |

<a id="planning"></a>
## Planning

`planDeterministically(requestText, projectName, capabilities, hasArchitecture, checks)` produces an `ExecutionPlan`: intent analysis (goal, motivation, outcomes, constraints, technologies, scope, priority, quality, security), ambiguities with a recorded assumption, requirements with acceptance criteria, and tasks with a capability, dependencies, inputs, outputs, reviews and verification methods. `CAPABILITY_RULES` maps request vocabulary to registry capabilities; a request that matches no rule becomes one general implementation task. A statement that asks for a specialist is classified as a consultation: it becomes one specialist task whose deliverable is the recorded answer, verified by the review rather than by a command, and written into the project context on completion.

<a id="scheduling"></a>
## Scheduling

`readyTasks` selects `PLANNED`/`READY` tasks whose dependencies are terminal and that have no assigned agent, ordered by priority then deadline then creation. `validateGraph` refuses an unknown dependency, a self-dependency and any cycle before a single task is persisted. `criticalPath` reports the longest dependency chain, which the plan event records so a reader can see what determines duration.

<a id="execution"></a>
## Execution

`runTask` walks one task through the lifecycle: select the profile, resolve the route, spawn the agent, create the worktree under a write lease, run the loop with policy-guarded tools, review, verify, integrate, complete or fail. Every tool call is guarded by `checkToolCall`, recorded as a tool execution, and fingerprinted so a repeated identical call raises a loop event before it consumes the whole budget. Usage is recorded per call: token counts always, USD only for a priced route. A configured project ceiling is re-checked after each recorded call.

An agent can coordinate mid-run through the runtime's two coordination tools. `answerHelp` records the `HELP_REQUEST`, runs a real helper instance of the profile serving the requested capability over the same workspace, records the `HELP_RESPONSE` and returns the answer to the requesting agent; `maxAgentsPerTask` bounds how many helpers one task may use. `recordHandoff` records the `HANDOFF` package, moves the task to the receiver's capability, releases the handing-off agent, and lets the successor start in the inherited workspace once the handing-off run settles. A task that was handed off is never reviewed or integrated by the agent that gave it away.

<a id="review-and-verification"></a>
## Review and verification

`reviewTask(input, delegate)` inspects changed files, tool executions, verification results and the completion report, raises severity-ranked findings with evidence, and optionally asks a delegated model reviewer for a second opinion grounded in the same evidence. `verifyTask` executes each criterion and records the command, exit code and output; `unverifiedCriteria` decides completion.

<a id="resources-and-git"></a>
## Resources and Git

`LeaseManager` grants read or write rights with a monotonic fencing token, refuses a conflicting owner and returns the conflict detail, expires and revokes rather than deleting, and answers `canAct` with the fencing check. The Git helpers create a worktree per agent, commit on submission, list changed files, merge without fast-forward, validate the integrated tree, roll back a failed validation, and report a textual conflict without touching the base branch.

<a id="decisions-cost-and-recovery"></a>
## Decisions, cost and recovery

`DecisionCenter.debate()` refuses an incomplete disagreement; `propose` records an autonomous decision immediately or waits on the user for a material one; `decide` records the outcome, the rationale and the rejected recommendation. `CostManager` prices every call against the route price and answers `budget` with the remaining headroom. `withRetry` refuses to repeat a non-idempotent operation after an unknown outcome, `backoffDelay` bounds the schedule, `CircuitBreaker` stops calling a failing provider, and `detectStall` reports heartbeat loss, progress loss and repeated invocations as separate evidence.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **One orchestrator per database.** Recovery restores state for one process; concurrent orchestrators over one platform database are not supported.
- **Semantic conflict resolution is recorded, not automated.** The integration manager detects overlap and a failed validation; resolving a semantic disagreement between two agents runs through a decision, not through an automatic merge heuristic.
- **Delegated review needs a route.** Without a delegation port the review runs on the recorded evidence alone and reports that no second opinion was obtained; it never invents one.

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

The orchestrator holds no module-level state: every map it keeps (abort controllers, stalled agents, lease chains) is an instance field, and the durable truth is in the store. That is what makes a restart safe and a test able to run two orchestrators side by side.

</details>
