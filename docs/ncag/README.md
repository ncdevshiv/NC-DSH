---
description: "NC-AG: the autonomous multi-agent orchestration platform built into the harness — architecture, deployment, operation, and verification."
kind: "subsystem"
---

# NC-AG

English | [中文](README.zh.md)

## Summary

NC-AG is an autonomous multi-agent orchestration platform: it accepts a complex instruction, analyzes intent, challenges contradictions, decomposes work into a task graph, spawns specialized agents in isolated Git worktrees, coordinates their communication and resource ownership, reviews and independently verifies their output, and delivers results backed by a recorded evidence trail. This page is the entry point: it maps the architecture, then points at the deployment, operation, security and verification guides.

## Table of Contents

- [What NC-AG does](#what-ncag-does)
- [Architecture](#architecture)
- [Run it](#run-it)
- [Guides](#guides)
- [What NC-AG will not do](#what-ncag-will-not-do)

-----

<a id="what-ncag-does"></a>
## What NC-AG does

A request enters through `ncag_request` (a harness tool) or the control-plane API. The platform then:

1. **Analyzes** the request into intent, constraints, technologies, quality expectations and security implications, and records every ambiguity. A material ambiguity is raised as a decision; an immaterial one becomes a recorded assumption.
2. **Normalizes** requirements with identifiers, priorities, acceptance criteria and verification methods, traceable to the request statement that produced them.
3. **Decomposes** into tasks with a capability, dependencies, inputs, expected outputs, review kinds and verification methods, then validates the dependency graph (no unknown dependency, no cycle).
4. **Schedules** ready tasks under the project concurrency ceiling and the critical path.
5. **Spawns** one agent instance per task from a registered profile, in its own Git worktree with a write lease that carries a fencing token.
6. **Executes** the agent: the loop asks the model for the next action, runs the tools the model requests under permission, containment and destructive-command policy, and feeds the results back.
7. **Reviews** with independent reviewers that inspect the recorded evidence; findings are severity-ranked and recorded, and a CRITICAL or HIGH finding rejects the task.
8. **Verifies** every acceptance criterion by executing the project-configured check command and recording the exact command, exit code and output as evidence.
9. **Integrates** the approved change through the integration manager, recording the changed files, the merge result and its validation report.
10. **Delivers** only when every criterion carries passing evidence; failed checks stay visible forever.

<a id="architecture"></a>
## Architecture

The platform is a set of packages with explicit interfaces. [`ncag-core`](../../packages/ncag/ncag-core/README.md) owns the vocabulary and the lifecycle machines, so no module can invent a status transition. [`ncag-store`](../../packages/ncag/ncag-store/README.md) is the only writer of durable state: one SQLite database, typed repositories, an ordered event stream, and a hash-chained audit log. [`ncag-runtime`](../../packages/ncag/ncag-runtime/README.md) executes one agent against a model port and a workspace, keeping the model call and the tool execution behind interfaces. [`ncag-orchestrator`](../../packages/ncag/ncag-orchestrator/README.md) makes the decisions: planning, scheduling, supervision, leases, integration, review, verification, decisions, cost and recovery. [`ncag-dashboard`](../../packages/ncag/ncag-dashboard/README.md) exposes the platform over HTTP and serves the operational UI. [`tool-ncag`](../../packages/ncag/tool-ncag/README.md) mounts the platform in the harness and registers the model-facing tools.

The orchestrator decides; the deterministic infrastructure enforces. Restarting the orchestrator restores every task, message, lease, decision and event from the database; a restart never silently loses track of running work, and no completed task is executed twice.

<a id="run-it"></a>
## Run it

```bash
# 1. Required configuration
export NCAG_DATABASE_PATH="$HOME/ncag/platform.db"
export NCAG_WORKSPACE_ROOT="$HOME/ncag/workspaces"
export NCAG_MODEL_PRICING='[{"provider":"deepseek","model":"deepseek-chat","inputPerMillion":0.27,"outputPerMillion":1.1}]'

# 2. Mount the platform beside the harness
dsh --profile web --patch packages/bundle/ncag/cordis.patch.yml
```

The `ncag` profile does the same composition in one name: `dsh --profile ncag web`. The control plane is an in-process HTTP service started by [`ncag-dashboard`](../../packages/ncag/ncag-dashboard/README.md), not a separate executable.

The shipped Web application also carries the platform as the **NC-AG mode** agent preset. Selecting it in the agent select composes the standard toolset plus the `ncag_project`, `ncag_request`, `ncag_status`, `ncag_events` and `ncag_decide` tools for that Session, reads the same `NCAG_*` configuration, and shares one orchestrator per database with any other mount of that platform.

<a id="guides"></a>
## Guides

| Guide | Covers |
|---|---|
| [Deployment](deployment.md) | Configuration, database, credentials, and the first project |
| [Operation](operation.md) | Control plane routes, operator controls, budget and recovery procedures |
| [Agent profiles](agent-profiles.md) | The sixteen built-in specializations and how to add one |
| [Protocols](protocols.md) | Task and agent lifecycles, message types, lease protocol, decision record, Git integration |
| [Security](security.md) | Permissions, workspace containment, untrusted content, audit integrity |
| [Verification](verification.md) | How completion is decided, what evidence is stored, and how to reproduce it |
| [Testing](testing.md) | What the suite proves, and how to run it |
| [Troubleshooting](troubleshooting.md) | The failures an operator actually sees, and what each one means |
| [Architecture decision records](decisions/README.md) | Why the platform is built this way |

<a id="what-ncag-will-not-do"></a>
## What NC-AG will not do

- **It will not invent a model route.** No provider, no price, no model means the task reports the missing route instead of running on an imagined provider.
- **It will not accept a verbal success report.** A task completes when the recorded verification evidence says its criteria passed.
- **It will not pretend a tool ran.** Every tool execution is recorded with its outcome; an unavailable shell, git or check command fails the call with the executable it looked for.
- **It will not hide a failed check.** Findings and evidence remain visible with their severity and status.
- **It will not silently follow an instruction found in retrieved content.** Workspace content is framed as data, and instruction-shaped markers are reported.
