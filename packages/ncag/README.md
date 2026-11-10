---
description: "The NC-AG package group: the autonomous multi-agent orchestration platform, from durable domain records to the control plane."
kind: "package-group"
---

# packages/ncag

English | [中文](README.zh.md)

## Summary

NC-AG accepts a complex instruction, analyzes intent, challenges contradictions, decomposes the work into a dependency graph, spawns specialized agents in isolated Git worktrees, coordinates their communication and resource ownership, reviews and independently verifies their output, and delivers a result backed by recorded evidence. This group holds the domain vocabulary and lifecycle machines, the store with its hash-chained audit log, the agent runtime, the orchestrator, the control plane, and the plugins that mount it beside the harness loop.

No module reports success without executed evidence, and none reports a provider or tool working before its configuration is verified.

## Table of Contents

- [Packages](#packages)
- [How the pieces fit](#how-the-pieces-fit)
- [Related documentation](#related-documentation)
- [Dev Note](#dev-note)

-----

<a id="packages"></a>
## Packages

| Package | Role | ctx key |
|---|---|---|
| [`ncag-core/`](ncag-core/README.md) | Domain vocabulary: branded identifiers, record types, the agent and task lifecycle machines, and the sixteen built-in agent profiles | library |
| [`ncag-store/`](ncag-store/README.md) | Durable platform store: SQLite schema and migrations, typed repositories, the platform event stream, and the hash-chained audit log | library |
| [`ncag-runtime/`](ncag-runtime/README.md) | Agent execution: provider-neutral model port, sandboxed workspace tools, prompt assembly, and the agent turn loop | library |
| [`ncag-orchestrator/`](ncag-orchestrator/README.md) | Orchestration: planning, DAG scheduling, agent lifecycle, leases, Git integration, review, verification, decisions, cost, recovery | library |
| [`ncag-dashboard/`](ncag-dashboard/README.md) | Control plane: HTTP API, server-sent events, the operational dashboard, and the platform process entry point | library |
| [`tool-ncag/`](tool-ncag/README.md) | Harness integration: the platform service plugin and the model-facing `ncag_*` tools | `ctx.ncag` |
| [`ncag-settings/`](ncag-settings/README.md) | Host settings namespace: per-tier model overrides, price overrides and optional limits edited from the Web settings page | `ctx.ncagSettings` |
| [`ncag/`](ncag/README.md) | Facade re-exporting the platform packages as one import | — |

<a id="how-the-pieces-fit"></a>
## How the pieces fit

A run follows one path. A user request enters through the harness tool `ncag_request` (or the control plane API), and the orchestrator plans it: the deterministic analyzer normalizes requirements and builds the task DAG. The scheduler takes tasks whose dependencies are terminal and has capacity, resolves a model route for the required capability, spawns a profile instance, and gives it an isolated worktree under a write lease. The agent runtime drives the model through its tools, recording every call. When the agent reports completion, the orchestrator runs the required reviews, then verification, then integration; only passing evidence lets a task reach `COMPLETED`. Usage is recorded per call: token counts always, plus USD when the route is priced. A project with a configured USD ceiling pauses and asks the owner when the ledger crosses it.

`ncag-store` is the only writer of durable state, and every action appends to a hash-chained audit log so a modified record breaks verification of every later record.

<a id="related-documentation"></a>
## Related documentation

- [NC-AG subsystem](../../docs/subsystems/ncag.md) — the record types, lifecycle state machines, communication and lease contracts, and the audit chain.
- [NC-AG guides](../../docs/ncag/README.md) — deployment, operation, security and verification procedures.
- [NC-AG architecture decision records](../../docs/ncag/decisions/README.md) — why the platform is built this way.
- [Bundle integration](../../packages/bundle/ncag/README.md) — the dsh bundle and the `ncag` profile.

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

NC-AG deliberately owns its own store rather than extending the harness session log: the platform runs many agents that are not harness Agents, and their durability must survive a harness restart without touching the session format. The single integration point with the harness is the model port, which routes through `ctx.llm`, so provider configuration, credentials and retries stay owned by the harness.

</details>
