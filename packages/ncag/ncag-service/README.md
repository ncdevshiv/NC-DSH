---
description: "The NC-AG platform service plugin: one orchestrator, its durable store, and its model port over the harness LLM service."
kind: "package-plugin"
---

# @deepseek-ai/dsh-ncag-service

English | [中文](README.zh.md)

## Summary

Mounts the NC-AG platform in a harness process as `ctx.ncag`. It creates one orchestrator from the validated plugin configuration, gives it a model port over `ctx.llm` when that service is present, starts it, and disposes it with the plugin lifetime. It reads the LLM and workspace services strictly with `ctx.get`, so a composition without them still boots; tasks then report the missing runtime capability instead of failing the whole profile. Every mount that resolves the same absolute database path shares one orchestrator: a second mount increments a reference count instead of opening a competing dispatcher, so two Sessions — or a preset plus the standalone bundle — cannot execute the same READY task twice. A `:memory:` database stays private to its mount.

## Table of Contents

- [Configuration](#configuration)
- [Service API](#service-api)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="configuration"></a>
## Configuration

| Field | Required | Meaning |
|---|---|---|
| `databasePath` | yes | absolute platform database path |
| `workspaceRoot` | yes | absolute directory holding agent workspaces |
| `modelPricing` | no | JSON price overrides; empty records tokens without USD |
| `defaultRoute` | no | the user's main model; follows `agentDefaultModel` when unset |
| `fastModel`, `standardModel`, `reasoningModel` | no | per-tier `provider/model` overrides from the NC-AG settings tab |
| `pricingOverrides` | no | settings-tab JSON price overrides |
| `uiMaxConcurrentAgents` | no | live-agent ceiling; 0 is unlimited |
| `uiMaxAgentsPerTask` | no | helper ceiling per task; 0 is unlimited |
| `uiMaxStepsPerTask` | no | model steps per task; 0 follows `maxStepsPerTask` |
| `uiMaxOutputTokens` | no | output ceiling per call; 0 follows `maxOutputTokens` |
| `uiMaxCostUsd` | no | project spend ceiling in USD; 0 is unlimited |
| `preferredProviders` | no | provider names in priority order |
| `leaseTtlMs` | no | lease duration, default 900000 |
| `maxConcurrentAgents` | no | default live-agent ceiling, default 4 |
| `maxStepsPerTask` | no | model steps per task, default 24 |
| `maxOutputTokens` | no | output ceiling per call, default 4096 |
| `healthIntervalMs` | no | observation interval, default 15000 |
| `stalledAfterMs` | no | progress timeout, default 120000 |
| `repeatedToolThreshold` | no | identical invocations before a loop event, default 4 |

A malformed `modelPricing` or `pricingOverrides` fails the load with the entry and the field that is wrong. An empty table is valid: token counts are always recorded, and only a priced route contributes USD.

<a id="service-api"></a>
## Service API

| Member | Meaning |
|---|---|
| `orchestrator` | the running `NcagOrchestrator` |
| `createProjectFromWorkspace(name, description)` | create a project anchored at the first registered workspace |

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **The control plane is not started here.** The platform service runs the orchestrator; serving the dashboard is the [`ncag-dashboard`](../ncag-dashboard/README.md) package's job, so a headless composition does not open a port.

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

The service and its tools live in separate packages because the Loader cannot mix a service default export with a function plugin in one module: one file would drop the other's namespace.

</details>
