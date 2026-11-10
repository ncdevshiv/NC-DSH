---
description: "Browser monitor and operator Remote namespace for the NC-AG platform: snapshot, timeline, decisions, and project/agent controls over the host-plane orchestrator."
kind: "package-reference"
---

# @deepseek-ai/dsh-api-ncag-controller

English | [中文](README.zh.md)

## Summary

Owns the `ncag` Remote namespace the Settings NC-AG page reads. Every method delegates to the orchestrator the host-plane `ctx.ncag` service owns, so this package adds a browser-safe wire projection and error translation — no platform behavior. It is mounted by the Web bundle so the page monitors the platform without a Session selecting NC-AG mode; the preset's own platform mount resolves to the same orchestrator because `ncag-service` deduplicates by database path.

## Table of Contents

- [Use this package](#use-this-package)
- [Understand the implementation](#understand-the-implementation)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)

-----

<a id="use-this-package"></a>
## Use this package

Mount the row beside the Web application, after the `ncag-settings` and `ncag` rows. The browser reads `ctx.remote.ncag` after `@deepseek-ai/dsh-api-remotes` mounts the generated contribution.

| Method | Meaning |
|---|---|
| `snapshot` | Projects with their tasks, agents, and counters, plus usage, audit-chain state, resolved routes, and the recent timeline |
| `trace` | One correlation's audit records, events, messages, and tool executions |
| `recentEvents` | Newest platform events, globally or for one project; the count is clamped by `maxEvents` |
| `decisions` | Every pending decision, or one project's decision history |
| `profiles` | Registered agent profiles and their capabilities |
| `createProject`, `submitRequest` | Open a workstream and submit work to it |
| `pauseProject`, `resumeProject` | Stop or restart dispatch for one project |
| `cancelTask`, `reassignTask` | Cancel a task or move it to a capability |
| `cancelAgent`, `instructAgent` | Stop one agent or steer it |
| `resolveDecision` | Record a human answer to one decision |

<a id="understand-the-implementation"></a>
## Understand the implementation

`NcagController` extends `TypertRemoteService` under the service key `ncagController` and the wire namespace `ncag`. Each method addresses the orchestrator or a repository, maps its failure onto `ncag/not-found` or `ncag/invalid`, and returns a wire record. The records are declared locally in `src/types.ts` rather than imported from the platform packages: a Host record satisfies them structurally, and the browser face must not pull SQLite, git, or model-port modules into its program. The namespace's failure vocabulary lives in `src/codes.ts`, which only the Host face includes.

The page polls `snapshot` and `recentEvents`; `trace` is read on demand. A missing addressed record is reported as `ncag/not-found` instead of an empty result, so the monitor never shows a quiet project that does not exist.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **Polling, not push.** The page re-reads on an interval; a `store.subscribe` stream would need a stream method plus a forwarded-event allowlist entry.
- **Loopback trust.** The Gateway treats the local user as the owner, so per-peer resource fencing is not applied to these reads.
- **Full records ride the wire.** Project, task, agent, and trace payloads are not slimmed; a large catalog may warrant a projection.
