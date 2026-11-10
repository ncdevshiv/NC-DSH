---
description: "The NC-AG tool plugin: the model-facing ncag_project, ncag_request, ncag_status and ncag_decide tools."
kind: "package-plugin"
---

# @deepseek-ai/dsh-tool-ncag

English | [中文](README.zh.md)

## Summary

Registers the five model-facing NC-AG tools on `ctx.tools`. The platform service is mounted by [`ncag-service`](../ncag-service/README.md); this plugin needs `tools` and `ncag` and registers on activation.

## Table of Contents

- [Tools](#tools)
- [Model Experience](#model-experience)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="tools"></a>
## Tools

| Tool | Operation | Returns |
|---|---|---|
| `ncag_project` | create a project (defaults to the current workspace when no repository path is given) | identifier, name, status, repository path |
| `ncag_request` | submit a request to a project and execute it | the created tasks with status and capability |
| `ncag_status` | read the platform snapshot, optionally one project | a text report of audit state, cost, tasks and agents |
| `ncag_decide` | answer a pending decision | the decision identifier, status and outcome |
| `ncag_events` | read one project's timeline | the ordered event kinds, timestamps and summaries |

Each tool calls the orchestrator method that performs the operation. `ncag_status` reports the audit-chain state first, because a broken chain invalidates every other claim in the report.

<a id="model-experience"></a>
## Model Experience

### Request context and condition

#### What the model sees

The five tool schemas and their descriptions. They introduce the platform to a model that has never used it: create a project, submit work, read status, answer a decision.

#### Token effect

Fixed: five schemas with short descriptions, independent of the conversation size.

#### KV Cache effect

Append-only in the tool declaration prefix: the schemas are static, so they never invalidate a cached prefix on their own.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **Five tools, not the whole control plane.** Pausing, resuming, cancelling and instructing are operator operations on the HTTP API; the model-facing surface exposes the five operations an agent plausibly needs mid-task.

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

The tools read `ctx.get('ncag')` at execution time rather than injecting it, so the tool plugin and the service plugin can load in either order without a pending dependency: the registry activates the tools as soon as the registry exists, and a call before the platform is mounted reports exactly that.

</details>
