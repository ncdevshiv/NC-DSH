---
description: "Process-wide tracing and metrics: bounded span and event rings over model calls, tool calls, turns, and plugin failures, served as a live JSON/SSE diagnostics endpoint."
kind: "package-reference"
---

# @deepseek-ai/dsh-telemetry-runtime

English | [中文](README.zh.md)

## Summary

Owns `ctx.telemetry`, the process-wide tracing surface. It subscribes to the harness's own seams — `llm/stream`, `tools/execute`, `session/event` — and records bounded spans, counters, per-Session and per-route rollups, crash records, and the Loader's fiber states. Nothing is written to a log file, and the only cost on a hot path is an array write into a fixed-capacity ring.

The rings are served three ways under `/api/diagnostics`, for any local agent or operator:

| Route | Meaning |
|---|---|
| `GET /api/diagnostics?events=N` | Complete snapshot as JSON |
| `GET /api/diagnostics/stream` | SSE: one `snapshot` frame, then every new span |
| `POST /api/diagnostics/client` | Browser span ingest (`[{ kind, name, durationMs, ok?, detail? }]`) |

## Table of Contents

- [Use this package](#use-this-package)
- [Understand the implementation](#understand-the-implementation)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)

-----

<a id="use-this-package"></a>
## Use this package

Mount the row on the host plane beside the Web application. Read live stats from an agent's shell with the URL the application prints:

```sh
curl -s "$DSH_WEB_URL/api/diagnostics" | jq
```

The snapshot carries the process facts (pid, uptime, RSS, heap), counters (`model.calls`, `tool.calls`, `turn.completed`, `span.slow`, `client.rpc`, …), latency p50/p95/max per span name, per-Session turns/steps/tool calls/tokens and its last route, per-route call/error counts with TTFT percentiles, the Loader rows and their fiber states, the newest crash records with their stack, and the newest spans. `health.ok` is false with a named reason when a plugin fiber failed or a crash landed in the last minute.

Other plugins may record their own spans through `ctx.telemetry.record`, `count`, `crash`, and `client`.

<a id="understand-the-implementation"></a>
## Understand the implementation

The service keeps one `Ring` per stream (spans/events, browser spans, crashes) sized from `Config`, one counter map, one duration bucket per span name, and one bucket per Session and per provider/model route. Recording is synchronous: a span push, a counter increment, and a fan-out to subscribers that are individually contained.

Instrumentation is subscription-only, so no package changes to be traced: the `llm/stream` waterfall is wrapped to time the first chunk and the whole call and to read usage chunks; `tools/execute` is timed around its own `next()`; `session/event` folds turn, step, and tool counts and measures turn duration from `turn/start` to `turn/end`.

The HTTP routes are registered on the host webserver and accept only loopback callers (or a request whose `Host` names a loopback address), because the snapshot names paths, models, and session ids.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **Host-side spans only.** Browser spans arrive when the client reports them; the shipped Gateway client reports every Remote round-trip, and deeper UI timings (first paint of a tool card, hover-to-open) still need per-surface instrumentation.
- **No persistence.** Rings are process-local and reset on restart; a durable trace store would need a storage unit and a retention policy.
- **No alerting.** `health.reasons` is the signal; nothing pages an operator.
