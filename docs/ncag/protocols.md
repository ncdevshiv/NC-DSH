---
description: "The NC-AG protocols: agent and task lifecycle machines, message types and delivery, the resource lease protocol, decisions, and Git integration."
kind: "reference"
---

# NC-AG protocols

English | [中文](protocols.zh.md)

## Summary

This page defines the state machines and protocols every NC-AG module obeys. The executable definitions live in [`ncag-core`](../../packages/ncag/ncag-core/README.md) and the enforcing modules named beside each protocol; a change to a lifecycle is a change to that package first.

## Table of Contents

- [Agent lifecycle](#agent-lifecycle)
- [Task lifecycle](#task-lifecycle)
- [Communication](#communication)
- [Resource leases](#resource-leases)
- [Git workspaces and integration](#git-workspaces-and-integration)
- [Decisions and debate](#decisions-and-debate)
- [Review and verification](#review-and-verification)

-----

<a id="agent-lifecycle"></a>
## Agent lifecycle

`CREATED → INITIALIZING → STARTING → IDLE ⇄ ASSIGNED ⇄ WORKING`, with the branch states `WAITING`, `BLOCKED`, `HELP_REQUESTED`, `REVIEWING`, `HANDOFF_PENDING`, `RECOVERING`, and the terminal path `STOPPING → STOPPED` or `FAILED`.

`FAILED` is the only state that may return to work, through `RECOVERING`. `STOPPED` and `STOPPED → anything` do not exist. [`transitionAgent`](../../packages/ncag/ncag-core/src/machines.ts) rejects every move outside the table, and [`AgentRegistry.transition`](../../packages/ncag/ncag-orchestrator/src/agents.ts) is the only writer of an agent status, so no module can move an agent into a state the machine does not permit.

A heartbeat records observation, not progress. `heartbeat(agentId, producedProgress)` advances `lastActivityAt` only when the agent produced durable progress (a tool result), which is what the stall detector reads.

<a id="task-lifecycle"></a>
## Task lifecycle

`CREATED → ANALYZING → PLANNED → WAITING_APPROVAL → READY → ASSIGNED → IN_PROGRESS → REVIEW_PENDING → VERIFICATION_PENDING → COMPLETED`, with `BLOCKED`, `REJECTED`, `FAILED`, `CANCELLED` as the alternate paths.

`COMPLETED` and `CANCELLED` are terminal. `REJECTED` may return to `IN_PROGRESS` (the reviewer sent it back) and `FAILED` may return to `READY` (the recovery manager reassigned it). Only `PLANNED` and `READY` tasks with all dependencies terminal and no assigned agent are schedulable.

<a id="communication"></a>
## Communication

Every message carries an identifier, a UTC timestamp, the sender (an agent or null for the orchestrator), the target, the project, the task, the type, a subject, content, context references, a correlation id, a priority, an acknowledgement requirement and a delivery state.

Types: `INFORMATION`, `TASK_UPDATE`, `HELP_REQUEST`, `HELP_RESPONSE`, `BLOCKER`, `RESOURCE_REQUEST`, `RESOURCE_RELEASE`, `CONTRACT_PROPOSAL`, `CONTRACT_CHANGE`, `ARCHITECTURE_PROPOSAL`, `ARCHITECTURE_CONFLICT`, `REVIEW_REQUEST`, `REVIEW_FINDING`, `HANDOFF`, `ESCALATION`, `TASK_COMPLETION`, `FAILURE`, `SYSTEM_EVENT`.

Delivery states are `PENDING → DELIVERED → ACKNOWLEDGED`, with `EXPIRED` and `DEAD_LETTERED`. A message is not delivered because a sender tried: the store records `PENDING` first, the delivery pump moves it, and an acknowledgement is a separate recorded fact keyed by message and agent, so a repeated acknowledgement does not double-count. Expiry is evaluated at delivery time, and a message that could not be delivered within its lifetime is dead-lettered with the reason.

Targets are an agent, a named channel, or the orchestrator. Channels exist so a project-wide announcement does not become an individual delivery.

An agent raises coordination requests through the two runtime tools it holds: `request_help` sends a `HELP_REQUEST` and the orchestrator answers it by running a real helper instance of the profile that serves the requested capability, returning the helper answer to the requesting agent and recording it as `HELP_RESPONSE`. `hand_off` sends a `HANDOFF` package and re-queues the task under the receiver's capability; the successor inherits the handing-off agent's workspace, so the same changed tree continues. Both tools run under the `communication:send` permission, the project's `maxAgentsPerTask` bounds how many helpers one task may use, and a handing-off agent stops after its own run settles rather than reviewing a delivery it no longer owns.

<a id="resource-leases"></a>
## Resource leases

A lease grants a read or write right over one repository-relative path to one agent for one task, with a monotonic fencing token. Write leases are exclusive; read leases may be shared when `allowSharedRead` is set.

Acquisition refuses when an active write lease or any active read lease holds the path, and returns a conflict naming the owner, the owning task, the stated reason, the expiry and whether an alternative workspace exists. Renewal refuses a lease that was superseded, because the fencing token it carries is no longer current. `canAct` is the check an executor performs before writing: an expired, released or superseded lease means the agent must not act.

Lease expiry and forced release are recorded transitions (`EXPIRED`, `REVOKED`) rather than silent deletion, and every agent keeps the evidence of what it held.

A refused acquisition is also durable. The waiting agent, its task, mode and stated reason are recorded in `lease_requests`, one row per project, path, agent and mode, so a repeated request keeps its original queue position. A grant clears that agent's entry; an operator reads the whole project queue through `LeaseManager.waiting(projectId)`, which is what the control plane's waiters route serves. The queue survives a restart because it is a table, not a promise held in memory.

<a id="git-workspaces-and-integration"></a>
## Git workspaces and integration

Each agent works in its own worktree on its own branch, created from the project base branch. The base commit is recorded. Submission commits the workspace, lists the changed files against the base commit, and records the change set.

Integration checks out the base branch, merges the submitted branch without fast-forward, then runs the configured validation. A textual merge that succeeds is not proof of a correct integration: the validation decides, and a failed validation rolls the merge back and reports it. A textual conflict aborts the merge, records the conflict and leaves the base branch untouched.

<a id="decisions-and-debate"></a>
## Decisions and debate

A decision record carries: identifier, project, task, category, author, problem, the debate case, the recommendation, the outcome, the rationale, the rejected recommendation, the consequences, the affected modules and tasks, the status, the creation instant, the decision instant and the review history.

Four categories: `autonomous` (the orchestrator decides and records immediately), `technical-recommendation` (decided by default, escalatable), `material-architectural` (waits for the user), `user-reserved` (waits for the user).

`DecisionCenter.debate()` refuses an incomplete case: it must state what was requested, what the platform believes, the exact conflict, the recommendation and the question, compare at least two alternatives on the same axes, and present evidence. That is why the decision center can always show a real disagreement instead of a one-sided notice.

A recorded override writes the user's choice into the project constraints, records the rejected recommendation and the rationale, unblocks the tasks waiting on that decision, announces the decision on the project channel, and continues. The platform does not reopen a settled decision without new information.

<a id="review-and-verification"></a>
## Review and verification

Review inspects the recorded evidence, never the agent's assertions: changed files, tool executions and verification results. A review with no changed files, a failed execution, an unverified criterion or an empty completion report raises a finding with severity, description, evidence, artifact, impact, recommended correction and verification method. `CRITICAL` and `HIGH` reject the task.

Verification executes the command each criterion names — the criterion's own `command`, or the project's configured check for its method — and records the exact command, exit code and captured output. Completion is computed from the recorded evidence: any criterion without passing evidence keeps the task out of `COMPLETED`.

A `manual-review` criterion with no configured command is answered from the recorded reviewer findings instead: it passes only when a review exists and no finding is `CRITICAL` or `HIGH`, and it fails with the finding list when the review rejected the work. A method with no command and no review evidence fails with the reason, because that check could not be executed.

A review rejection consumes one review cycle and records it on the task. A task that exceeds `maxReviewCycles` stops reworking and fails with an `ESCALATION` message carrying every open finding, so a rejected delivery cannot loop forever.
