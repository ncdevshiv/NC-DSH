---
description: "Operating NC-AG: the control-plane API, the operator controls, budget handling, and the recovery procedures for a running platform."
kind: "guide"
---

# NC-AG operation

English | [中文](operation.zh.md)

## Summary

The control plane is an in-process HTTP service owned by [`ncag-dashboard`](../../packages/ncag/ncag-dashboard/README.md). It serves one dashboard document, a server-sent event stream, and the API the dashboard itself calls. Every route performs the operation it advertises: the dashboard renders only returned data, and every control calls the same orchestrator method the model-facing tools call.

## Table of Contents

- [Start and stop](#start-and-stop)
- [API reference](#api-reference)
- [Operator procedures](#operator-procedures)
- [Budget handling](#budget-handling)
- [Recovery procedures](#recovery-procedures)
- [Health checks](#health-checks)

-----

<a id="start-and-stop"></a>
## Start and stop

The control plane starts with the platform service, on `NCAG_PORT` (0 selects a free port) and `NCAG_HOST`. A standalone operator process can start the same platform over a database directly:

```ts
import { runPlatform } from '@deepseek-ai/dsh-ncag-dashboard'

const platform = await runPlatform(process.env, llmPort, routeRegistry)
console.log(platform.url)
await platform.stop()
```

`llmPort` is the [`ncag-runtime`](../../packages/ncag/ncag-runtime/README.md) model port. When it is `null` the platform still starts, and every task that needs a model reports the missing route instead of executing.

Stopping is graceful: the orchestrator checkpoints execution state, aborts in-flight agents, and closes the database. Calling `stop` twice is a no-op.

<a id="api-reference"></a>
## API reference

Every route below requires `Authorization: Bearer $NCAG_TOKEN` when a token is configured; the dashboard passes it as `?token=` for the event stream, because a browser `EventSource` cannot set headers.

| Method and path | Operation |
|---|---|
| `GET /` | The dashboard document |
| `GET /events` | Server-sent stream of platform events |
| `GET /api/health` | Audit-chain state, project count, live agents; `500` when the chain is broken |
| `GET /api/snapshot` | Full platform snapshot: projects with tasks and agents, usage, events, audit state, routes, profile count |
| `GET /api/projects` | Every project |
| `POST /api/projects` | Create a project (`name`, `description`, `repositoryPath`, optional `baseBranch`, `instructions`) |
| `POST /api/projects/:id/requests` | Submit a request; returns the plan and the created tasks |
| `GET /api/projects/:id/tasks` | A project's tasks |
| `GET /api/projects/:id/agents` | A project's agent instances |
| `GET /api/projects/:id/messages` | The project's durable message log |
| `GET /api/projects/:id/decisions` | The project's decision record |
| `GET /api/projects/:id/leases` | Active resource leases |
| `GET /api/projects/:id/changesets` | Submitted and integrated change sets |
| `GET /api/projects/:id/events` | The project's recent platform events |
| `GET /api/tasks/:id/changesets` | The change sets submitted for one task |
| `GET /api/tasks/:id/evidence` | The verification evidence recorded for one task, with each criterion's command, exit code and output |
| `GET /api/tasks/:id/findings` | The review findings raised for one task, with severity, artifact and status |
| `POST /api/projects/:id/pause` | Pause the project, recording the reason |
| `POST /api/projects/:id/resume` | Resume and dispatch the work the project already holds |
| `POST /api/tasks/:id/cancel` | Cancel a task and release its agent's leases |
| `POST /api/decisions/:id/resolve` | Record a decision outcome, or the user's override plus its rationale |
| `POST /api/agents/:id/instruct` | Send an operator instruction to one agent through the durable message log |
| `POST /api/agents/:id/cancel` | Cancel one live agent: abort it, release its leases, return its task to the queue |
| `POST /api/tasks/:id/reassign` | Re-queue one task under another capability (`capability`, optional `reason`) |
| `GET /api/projects/:id/waiters` | The agents queued behind each leased path, with their task and stated reason |
| `GET /api/projects/:id/{profiles,requests,requirements,reviews}` | Registered profiles, the project's requests, their normalized requirements, and the review runs |

A missing or wrong token is `401`. An unknown route is `404` with the path that was requested. A failed operation is `500` with the message the orchestrator raised; the platform never reports success for an operation that threw.

<a id="operator-procedures"></a>
## Operator procedures

**Pause before maintenance.** `POST /api/projects/:id/pause` stops new dispatch; running tasks settle. Resume with `POST .../resume`, which immediately dispatches the ready work the project already holds.

**Stop a runaway task.** `POST /api/tasks/:id/cancel` aborts the agent, releases its leases, and records the cancellation reason on the task and in the audit chain. Cancelling a task does not delete its work; the workspace and the change set remain inspectable.

**Send an instruction.** `POST /api/agents/:id/instruct` writes the text into the project message log addressed to that agent, with an acknowledgement requirement. It reaches the agent on its next step, and the delivery is visible in the timeline.

**Answer a decision.** The decision center shows the structured disagreement: what was requested, what the platform believes, the conflict, the recommendation, the compared alternatives, the evidence and the exact question. Accepting records the recommendation; recording an alternative records the rejected recommendation, the user's rationale, a project constraint, and releases the tasks the decision was blocking.

<a id="budget-handling"></a>
## Budget handling

Every model call is priced against the configured route price and appended to the usage ledger. After each recorded call the orchestrator re-checks the project ceiling, so a project that crosses `maxCostUsd` is paused as soon as the call that crossed it completes, not at the next dispatch. It then proposes a `user-reserved` decision asking for more spend. Nothing executes until that decision is answered, and the project resumes through `resumeProject` or an explicit approval flow.

Budget state is durable: `GET /api/snapshot` reports the spent amount per project, and the pause survives a restart.

<a id="recovery-procedures"></a>
## Recovery procedures

**A stalled agent.** The health observer reports an agent whose heartbeat or durable progress stopped. The orchestrator aborts its work, releases its leases, records the evidence, returns the task to the ready queue with one more attempt, and stops the agent through the lifecycle machine. Above `maxRetries` the task fails with the reason and remains visible.

**A repeated tool call.** `NCAG_REPEATED_TOOL_THRESHOLD` identical invocations (same tool, same arguments) by one agent emit a `recovery/stall-detected` event with the fingerprint and the attempt count, so a loop is visible before it consumes the whole step budget.

**A provider failure.** Model calls fail through the harness LLM service, which owns provider retry policy. NC-AG records the failure on the agent, and the recovery path treats an agent whose run threw exactly like a stalled agent.

**A restart.** `recoverAfterRestart` runs at startup: it retires agents whose task is finished, returns an interrupted task to the ready queue, retries pending deliveries, and records one `restart-recovered` audit entry. It never re-runs completed work; the transition count after a restart equals the count before it.

<a id="health-checks"></a>
## Health checks

`GET /api/health` returns `status: "ok"` with the audit state when the chain verifies, and `status: "audit-chain-broken"` with HTTP `500` and the first broken sequence otherwise. Point an orchestrator or a monitoring probe at it. The chain breaks only when a record was modified or removed outside the platform.
