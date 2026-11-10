---
description: "The NC-AG facade: one import for the platform service, the orchestration libraries, the store and the control plane."
kind: "package-library"
---

# @deepseek-ai/dsh-ncag

English | [中文](README.zh.md)

## Summary

The facade re-exports the platform packages so a consumer imports one name: the service plugin, the tool factory, the orchestrator, the domain vocabulary, the store, the runtime and the control plane. It adds no behavior and no configuration of its own.

## Table of Contents

- [What it re-exports](#what-it-re-exports)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)

-----

<a id="what-it-re-exports"></a>
## What it re-exports

| Source | Contents |
|---|---|
| `@deepseek-ai/dsh-tool-ncag` | `apply`, `inject`, `name`, `createNcagTools` |
| `@deepseek-ai/dsh-ncag-service` | the default service class, `Config`, `name`, `inject` |
| `@deepseek-ai/dsh-ncag-orchestrator` | the orchestrator, planner, DAG, review, verify, leases, Git, decisions, cost, recovery |
| `@deepseek-ai/dsh-ncag-core` | identifiers, records, lifecycle machines, profiles |
| `@deepseek-ai/dsh-ncag-store` | `NcagStore`, repositories, audit, schema |
| `@deepseek-ai/dsh-ncag-runtime` | ports, tools, prompt assembly, the loop, the harness model port |
| `@deepseek-ai/dsh-ncag-dashboard` | `startDashboard`, `runPlatform`, the dashboard document |

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **One name for every face.** A consumer that wants the service plugin imports the facade, which also loads the libraries; a consumer that wants only the data model imports `ncag-core` directly.

