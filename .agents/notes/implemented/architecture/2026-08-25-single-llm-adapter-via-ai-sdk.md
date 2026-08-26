# Agent Note: one LLM adapter over the ai-sidecar child process

Status: implemented

English | [中文](2026-08-25-single-llm-adapter-via-ai-sdk.zh.md)

> Supersedes [the twin LLM adapters](2026-06-13-twin-llm-adapters.md) and [provider-routed LLM adapters](2026-07-14-provider-routed-llm-adapters.md) in part: the harness ships one adapter implementation again, and the per-route wire-protocol profile it replaced is gone. [Mandatory app-attribution headers](2026-06-21-mandatory-app-attribution-headers.md) no longer bind adapters whose transport a separate process owns.

## Problem

Two in-house adapter packages owned the provider wire: `dsh-llm-deepseek` (direct DeepSeek HTTP) and `dsh-llm-pi-ai` (multi-provider through the `@earendil-works/pi-ai` library). Every new provider capability had to land twice or be routed through a per-route `api` protocol field, a compat-switch profile (`PiAiCompatProfile`), and adapter-owned SSE parsing that duplicated what a multi-provider SDK already maintains. The vendor dependency also pinned a third-party release cadence onto harness model behavior. The AI SDK Rust workspace (multi-provider registry with native Anthropic and Gemini clients plus an OpenAI-compatible client) existed but exposed no process interface a Node harness could drive.

## Decision

One package, `@deepseek-ai/dsh-llm-ai-sdk`, implements `LlmAdapter` for every registered route; nothing else in the harness opens a provider connection. It drives the AI SDK through one long-lived `ai-sidecar` child process speaking newline-delimited JSON-RPC 2.0 over stdio (protocol version 1): `initialize`, `configure` (whole provider-generation replace), `provider.list`, `model.list`, `model.discover`, `chat.stream`, and the `chat/event`/`chat/done`/`stream.cancel` notifications. The sidecar source of truth is `crates/ai-sidecar`; deployments select the executable with `llm-ai-sdk.binaryPath` or `$DSH_AI_SDK_SIDECAR`, and an unset path fails the first request loud with `CONFIG`.

The route id selects the wire dialect inside the sidecar: `anthropic` and `google` are native; `openai`, `openrouter`, and `ollama` are OpenAI-compatible with built-in default endpoints; any other id is OpenAI-compatible against its configured `baseURL`. A profile's optional `api` field overrides the derivation explicitly ([model discovery and route dialects](../feature/2026-08-26-model-discovery-and-route-dialects.md)). Omitting `providers` keeps the default `deepseek-official` route against the public endpoint.

Connection facts stay unfrozen: `resolveAdapterOptions()` re-resolves the layered config once per operation, credentials resolve per request through `ctx.credentials` (then trusted environment layers), and the adapter pushes one `configure` generation to the child only when the resolved credential/endpoint set changed — an in-flight stream keeps its starting facts. Settings hot-replace runs through the existing `installSettingsSection` seam with the namespace `llm-ai-sdk`; an invalid beyond-schema snapshot keeps the last good facts and logs once. The old packages, their composition rows, and `.github/workflows/pi-ai-provider-e2e.yml` are deleted; real-provider coverage moves to `packages/llm/llm-ai-sdk/tests/adapter.e2e.ts`, which self-skips without `DSH_AI_SDK_SIDECAR`.

## Alternatives considered

**Keep both adapters and route capabilities to their intersection.** Rejected: every new provider capability landed twice or behind a compat-switch profile, and the vendor pin coupled harness model behavior to a third-party release cadence — the duplication this decision deletes.

**Drive the AI SDK in-process through a Node↔Rust bridge instead of a sidecar.** Rejected for this iteration: a native addon couples the harness build to the Rust toolchain on every developer machine and in every deployment, while the stdio JSON-RPC child keeps the boundary a release binary any deployment can rebuild or replace.

**Keep the per-route `api` protocol field as the dialect selector.** Superseded in part by [model discovery and route dialects](../feature/2026-08-26-model-discovery-and-route-dialects.md): the field returns as an explicit override on the single adapter's profile, while route-id derivation remains the default.

## Consequences

- The bundle row shape changed from two adapter entries with per-route protocol/compat fields to one entry whose `providers` dict carries `displayName`/`apiKeyEnv`/`baseURL`/`api`/`models`/caps/efforts; `binaryPath` is the one new deployment fact every configuration must supply.
- Provider HTTP headers are the sidecar's property: requests carry no `attributionHeaders()`, no `x-deepseek-harness-user-id`, and no compaction marker today. Any future header requirement lands in the sidecar protocol.
- Reasoning effort collapses on the wire (`max` → `high`) because the protocol defines three levels, and an explicit `off` omits the wire field rather than forcing thinking off; prior-turn reasoning travels as plain assistant text, so provider-native thinking signatures do not round-trip.
- Image payloads over a route's `maxRequestImageBytes` reject the request instead of degrading to placeholders.
- The web Models page keeps key management, model-catalog editing, custom-provider creation, and live endpoint interrogation; [model discovery and route dialects](../feature/2026-08-26-model-discovery-and-route-dialects.md) owns how those activated.
