---
description: "The NC-AG test suite: what each spec proves, how the scripted model drives the real execution path, and how to run the suite."
kind: "guide"
---

# NC-AG testing

English | [中文](testing.zh.md)

## Summary

The NC-AG suite exercises the real execution path: real Git worktrees, real file tools, real leases, real review and verification, real integration, and one SQLite database per harness. Only the model call is scripted, because that is the one boundary that costs money and needs a provider. This page lists what each spec proves and how to run it.

## Table of Contents

- [Run the suite](#run-the-suite)
- [The scripted model](#the-scripted-model)
- [Scenario coverage](#scenario-coverage)
- [What the suite does not cover](#what-the-suite-does-not-cover)

-----

<a id="run-the-suite"></a>
## Run the suite

```bash
npx vitest run packages/ncag
```

| Spec | Proves |
|---|---|
| `packages/ncag/ncag-orchestrator/tests/scenario-a.spec.ts` | A request is planned, executed in a real worktree, reviewed, verified with an executed check, and recorded on the audit chain as a completed task |
| `packages/ncag/ncag-orchestrator/tests/scenarios.spec.ts` | Two concurrent projects stay isolated; an occupied path refuses a second writer and names the owner; a repeated tool call raises a loop event; a material disagreement is recorded and a user override is applied; a failing verification stops the task; a restart preserves state without duplicate work; a cost ceiling pauses the project and asks for a decision; instruction-shaped workspace content is framed as data; the health sweep retires finished agents without reassigning a completed task; an approved change is integrated onto the base branch without relying on the host's global git identity; a consultation is answered by the named specialist and its answer becomes project context |
| `packages/ncag/ncag-orchestrator/tests/operators.spec.ts` | Cancelling a live agent returns its task to the queue; reassigning a task moves it to another capability with that profile's permissions; a user decision unblocks the affected tasks whether the recommendation was accepted or overridden |
| `packages/ncag/ncag-dashboard/tests/control-plane.spec.ts` | The dashboard is served, unauthenticated calls are refused, API operations create a project and read it back, and every control in the document calls a route the server serves |

Each spec starts its own temporary repository and database, and removes both on teardown. A spec that passes only alone is a defect in the spec.

<a id="the-scripted-model"></a>

## The scripted model

`ScriptedModel` implements the `LlmPort` the runtime consumes. A script is a list of steps per agent identity phrase; each step is either a tool call or the final answer. The orchestrator drives the same loop it uses in production, so every script step executes a real tool against a real workspace.

<a id="scenario-coverage"></a>

## Scenario coverage

| Scenario | Spec |
|---|---|
| A simple request | `scenario-a.spec.ts` |
| A full-stack request | the capability rules select frontend, api, database and backend tasks with a dependency graph |
| Multiple concurrent requests | `scenarios.spec.ts` B/C |
| Shared-file conflict | `scenarios.spec.ts` D proves the lease refuses a second writer; `operators.spec.ts` runs two worktrees editing the same line through the real integration manager and proves the second change set is reported as a textual `CONFLICT` while the base branch keeps only the first change |
| Agent failure and stuck agent | `scenarios.spec.ts` E/F |
| Helper request | `scenarios.spec.ts` G: a working agent calls `request_help`, a real helper instance of the requested specialization runs the loop over the workspace, and its answer returns inside the requesting agent's run |
| Helper budget | `scenarios.spec.ts` G2: a second help request over the project's `maxAgentsPerTask` is refused with the ceiling named, so specialists cannot be spawned recursively |
| Agent handoff | `scenarios.spec.ts` G3: an agent calls `hand_off`, the `HANDOFF` package is recorded, and the successor continues in the inherited workspace, editing the file its predecessor wrote |
| User disagreement and override | `scenarios.spec.ts` H/I |
| Review disagreement | `scenarios.spec.ts` J |
| Orchestrator restart | `scenarios.spec.ts` K |
| Budget exhaustion | `scenarios.spec.ts` L |
| Long-running resume | `scenarios.spec.ts` M interrupts a task while an agent is working on it, restarts the platform, and proves the interrupted task returns to the queue with its dependency intact while the finished task gains no new transition and the resumed pass completes it exactly once |
| Untrusted instructions | `scenarios.spec.ts` N |
| Finished-agent sweep and integration | `scenarios.spec.ts` O/P |
| Consultation request | `scenarios.spec.ts` Q |
| Final delivery | `scenario-a.spec.ts` verification evidence |

<a id="what-the-suite-does-not-cover"></a>

## What the suite does not cover

- **A real model provider.** The suite runs without credentials by design. Real-provider behavior is covered by the repository end-to-end suite, which self-skips without a key.
- **A browser-driven dashboard interaction.** The control-plane spec calls the API the dashboard calls; the document itself is asserted for its served content, not for rendered pixels.
