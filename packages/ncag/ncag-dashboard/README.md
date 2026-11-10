---
description: "The NC-AG control plane: the HTTP API, the server-sent event stream, the operational dashboard, and the platform process entry point."
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-dashboard

English | [中文](README.zh.md)

## Summary

The control plane. `startDashboard(orchestrator, config)` serves the dashboard document, the event stream and the API on one HTTP server; `runPlatform(environment, llm, registry)` starts the orchestrator and the control plane together for a standalone deployment. Every route calls the orchestrator method that performs the operation, and the dashboard renders only data the API returned.

## Table of Contents

- [Routes](#routes)
- [The dashboard](#the-dashboard)
- [The event stream](#the-event-stream)
- [Authentication](#authentication)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="routes"></a>
## Routes

| Method and path | Operation |
|---|---|
| `GET /` | the dashboard document |
| `GET /events` | server-sent platform events |
| `GET /api/health` | audit-chain state, project and live-agent counts |
| `GET /api/snapshot` | projects with tasks and agents, usage, events, routes, profile count |
| `GET/POST /api/projects` | list / create |
| `POST /api/projects/:id/requests` | submit a request; returns the plan and tasks |
| `GET /api/projects/:id/{tasks,agents,messages,decisions,leases,waiters,changesets,events}` | project views, including the agents queued behind a leased path |
| `GET /api/projects/:id/{profiles,requests,requirements,reviews}` | registered profiles, the project's requests, their normalized requirements, and the review runs |
| `GET /api/tasks/:id/{changesets,evidence,findings}` | one task's change sets, verification evidence and review findings |
| `POST /api/projects/:id/{pause,resume}` | project control |
| `POST /api/tasks/:id/cancel` | cancel and release |
| `POST /api/tasks/:id/reassign` | re-queue the task under another capability |
| `POST /api/agents/:id/cancel` | cancel one live agent and return its task to the queue |
| `POST /api/decisions/:id/resolve` | record an outcome or an override |
| `POST /api/agents/:id/instruct` | operator instruction |

Request bodies are JSON, capped at 1 MB. A body that is not valid JSON is a `500` with the parse error; an operation that throws is a `500` with the orchestrator's reason. The server never reports success for a call that failed.

<a id="the-dashboard"></a>
## The dashboard

One document with no client build step: it fetches `/api/snapshot`, renders projects, requirements, tasks, the live agent monitor, leases with their wait queue, review runs, usage, the timeline and the audit state, and streams events over `EventSource` to refresh. Controls exist for creating a project, submitting a request, pausing and resuming, cancelling a task, cancelling an agent, reassigning a task, instructing an agent, inspecting a task's change sets, evidence and findings, filtering the message log, reading the decision history, and answering a decision — including the full debate case when one is recorded. Every control calls the API route documented above, and the control-plane spec asserts that each control's route is one the server serves.

<a id="the-event-stream"></a>
## The event stream

`/events` writes `data: <event JSON>` frames. The server subscribes to `store.subscribe` once and forwards to every connected response, removing each stream on `request close`. A disconnected client does not stop the platform; the dashboard reconnects automatically.

<a id="authentication"></a>
## Authentication

`token` is the bearer token required on every API call when set. The event stream accepts it as a `?token=` query parameter because `EventSource` cannot set headers. With no token the API is open, which is why `host` defaults to `127.0.0.1`.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **No per-user authorization.** One token gates the whole control plane. Multi-operator deployments need user identities, which live in the harness remote-access layer rather than here.
- **The dashboard is one document.** It is deliberately buildless; a richer editor for profiles and budgets belongs in the harness Settings surface once a consumer needs it.

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

`ui.ts` exports the document as a string constant so the package ships no browser build. Route handlers live in `routes.ts` and receive a `RouteContext`; adding a route means adding one branch there and one control in the dashboard, and both must call the same orchestrator method.

</details>
