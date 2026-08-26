# Agent Note: model discovery and route dialects on the single adapter

Status: implemented

English | [中文](2026-08-26-model-discovery-and-route-dialects.zh.md)

> Completes [the single-adapter migration](../architecture/2026-08-25-single-llm-adapter-via-ai-sdk.md): the deferred custom-provider entry point and endpoint interrogation are shipped, and the sidecar client's process lifecycle is closed.

## Problem

The migration deleted the per-route `api` protocol field, which was also the schema fact the Models page read to offer custom-provider creation — so that entry point rendered permanently disabled, and no sidecar method existed to interrogate an endpoint configuration had not stored yet. Separately, `AiSidecarClient` spawned its child into a local variable and never stored it: `dispose()` could not kill anything, a failed initialize left a healthy process behind while clearing the memo that prevented a second spawn, and every test run or plugin reload leaked live node processes — the accumulation the user observed as hundreds of orphaned `node.exe` entries.

## Decision

A provider profile accepts an optional `api` dialect — one of `openai-compatible`, `anthropic`, or `google`. Omission derives from the route id exactly as the sidecar always derived it, so existing compositions are unchanged; an explicit value lets a custom URL speak a native format. The derivation lives once (`defaultApiOf`) and mirrors the SDK's id-based selection.

The sidecar protocol gains `model.discover`: given `{api_key?, base_url?, api?}`, it builds a transient provider for one listing call without touching the configured generation, so probing a draft cannot disturb in-flight streams. The configure path registers adapters under the ROUTE name (`AiClientBuilder.provider_as`), because two routes may share one wire format and references resolve as `route:model`; an unknown dialect fails loud with a typed configuration error before any network call. The release binary must be rebuilt to serve protocol surface.

The adapter registers `ctx.llm.registerModelDiscovery('llm-ai-sdk', …)`: a draft naming only a known route is answered from that route's advisory catalog with no sidecar call; anything else resolves the dialect (draft override → route profile → route-id table), falls back to the route's stored credential when the draft carries none, requires a base URL for OpenAI-compatible interrogations, maps wire rows onto `LlmDiscoveredModel` at the boundary, and holds the one-shot key nowhere afterwards.

`AiSidecarClient` retains its child before any await; teardown is one idempotent, generation-aware path shared by exit, spawn failure, failed initialize, and disposal — a dying child's exit event cannot tear down a successor spawned after it. A new readonly `pid` accessor gives tests and diagnostics the lifecycle observation seam.

On the Models page, `protocolChoices()` reads the dialect union out of the restored schema field, so the create card enables exactly when the namespace mounts; a hand-declared route edits its display name and dialect through the same fields the create card asked for; every route edits models through the interrogation-aware list editor. Settings attach re-derives the directory through the registration's atomic `replace` handle — a bare re-registration collided with already-declared ids and tore the settings wiring down mid-attach, which is why hot-reload never landed.

## Alternatives considered

**Keep the entry point disabled and document `settings.yaml` as the declaration path.** The migration's own recorded choice, reversed here: the page is the product surface for adding providers, and a permanently disabled primary action is not a documented limitation but a missing feature.

**Infer the dialect in the browser from the endpoint URL.** Lost because it duplicates the SDK's selection table in a second language and drifts from it; the explicit field travels to the sidecar, which owns the truth.

**Fix the leak by killing in `dispose()` only.** Insufficient on both counts: the respawn-per-failed-initialize loop needs the initialize-failure teardown, and without generation awareness the kill of a replaced child tears down its successor's transport.

## Consequences

- Deployments must rebuild or re-fetch `ai-sidecar` to serve `model.discover`, the `configure` dialect passthrough, and route-keyed registration; older binaries fail discovery loud rather than silently degrading.
- Every llm-ai-sdk client generation now terminates its child: failed initializes, disposals, and exits each leave zero live processes, so watch-mode test runs and plugin reloads no longer accumulate orphans.
- Discovery cancellation rides the transport's existing limits rather than a request signal: an interrogation settles at the 120-second JSON-RPC ceiling, matching every other sidecar call.
- The real-API cloud e2e remains key-gated as before; local coverage runs the full stack against the real release binary and a scripted OpenAI-compatible HTTP server, including a live listing round trip.

## Testing

`bun run vitest run packages/llm/llm-ai-sdk packages/client/ui-settings-models packages/settings packages/llm/llm` is green (683 passing; one committed Windows-host symlink test fails on machines without symlink privilege, unrelated). `DSH_AI_SDK_SIDECAR=<release binary> bun run vitest run --config vitest.e2e.config.ts packages/llm/llm-ai-sdk` passes both real-binary cases: streamed completion and unsaved-endpoint discovery with stored-key authentication. `bun run typecheck` reports zero errors repository-wide.
