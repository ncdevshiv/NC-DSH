# @deepseek-ai/dsh-llm-ai-sdk

English | [中文](README.zh.md)

The harness's single LLM adapter: one `AiSdkAdapter` instance serves every configured provider route — DeepSeek official, OpenRouter, Ollama, Anthropic, Gemini, any OpenAI-compatible gateway — through one long-lived `ai-sidecar` child process. The sidecar speaks newline-delimited JSON-RPC 2.0 over stdio (protocol version 1) and owns every provider wire protocol; no other harness package opens a provider connection.

The adapter is transport-only. Routes arrive through a thunk resolved once per operation, the bearer key resolves per request, and the sidecar re-configures only when the resolved credential/endpoint generation changes, so an in-flight stream keeps the facts it started with. The package root exposes the Cordis plugin contract (`apply`, `resolveAdapterOptions`) plus the client, adapter, and translation helpers; the sidecar binary itself is external and selected entirely by configuration.

## Config

| Key | Type | Default | Notes |
|---|---|---|---|
| `binaryPath` | string | `$DSH_AI_SDK_SIDECAR` | Absolute path of the `ai-sidecar` executable. Unset everywhere, the first request fails loud with `CONFIG`; the web Models page stays reachable so the path can be configured after boot. |
| `providers` | dict of `ProviderProfile` | default DeepSeek route | Routes keyed by route id; omission keeps `deepseek-official` against the public endpoint with the `DEEPSEEK_API_KEY` reference. |
| `streamIdleTimeoutMs` | number | `300000` | Maximum provider idle time while one stream read is outstanding. |
| `retryPolicy` | `RetryPolicyConfig` | normal mode defaults | Shared model-request retry policy, registered as provider metadata and executed by `dsh-llm-retry`. |

Each `ProviderProfile` accepts `displayName`, `apiKeyEnv`, `baseURL`, `api`, `models`, `maxTokens` (default 256,000), `defaultContextWindow` (default 1,000,000), `reasoningEfforts`, `reasoningEffort` (default `high`), and `maxRequestImageBytes` (default 20 MiB). An omitted `apiKeyEnv` resolves the reference `<ROUTE>_API_KEY` (route key uppercased).

`api` names the wire dialect explicitly — one of `openai-compatible`, `anthropic`, or `google`. Omission derives it from the route id exactly as the table below records; declare it when a custom endpoint speaks a native format its URL cannot identify (an Anthropic-format relay on your own domain), or to pin the OpenAI-compatible family for a route id that would otherwise resolve natively.

```yaml
- id: llm-ai-sdk
  name: '@deepseek-ai/dsh-llm-ai-sdk'
  config:
    binaryPath: /opt/ai-sdk/bin/ai-sidecar # or $DSH_AI_SDK_SIDECAR in the launching environment
    providers:
      deepseek-official:                   # default route kept explicit here
        apiKeyEnv: DEEPSEEK_API_KEY
        baseURL: https://api.deepseek.com
        models:
          - id: deepseek-v4-flash
            name: DeepSeek-V4-Flash
          - id: deepseek-vision
            inputModalities: [text, image]
      anthropic:
        apiKeyEnv: ANTHROPIC_API_KEY       # baseURL omitted: native SDK default applies
        models:
          - id: claude-sonnet
            contextWindow: 200000
      my-gateway:                          # custom OpenAI-compatible gateway
        apiKeyEnv: MY_GATEWAY_KEY
        baseURL: https://gateway.internal/v1
        maxRequestImageBytes: 10485760
      acme-relay:                          # native Anthropic format behind a custom URL
        api: anthropic
        apiKeyEnv: ACME_RELAY_KEY
        baseURL: https://relay.acme.internal
```

## Provider routes

The route id selects both the composition entry and the wire dialect the sidecar applies; an explicit `api` overrides the derivation:

| Route id | Wire dialect | `baseURL` |
|---|---|---|
| `anthropic` | native Anthropic | optional |
| `google` | native Gemini | optional |
| `openai` | OpenAI-compatible | optional (`https://api.openai.com/v1`) |
| `openrouter` | OpenAI-compatible | optional (`https://openrouter.ai/api/v1`) |
| `ollama` | OpenAI-compatible | optional (`http://localhost:11434/v1`) |
| any other id | OpenAI-compatible | required |

Catalog entries are advisory: unlisted model ids still pass through unchanged, and `ctx.llm.listModels(route)` exposes them to discovery consumers such as ACP editors and the web selector. An omitted entry name defaults to its id, and omitted `inputModalities` means text only — declaring `[text, image]` is what admits image input on that model, and an uncatalogued endpoint is treated as text-only rather than assumed capable.

## Model discovery

The plugin serves `ctx.llm.discoverModels('llm-ai-sdk', request)` so configuration surfaces can interrogate an endpoint a draft is still editing. A draft naming only an existing route asks "what do you know" and is answered from that route's advisory catalog without touching the sidecar; a draft carrying an endpoint (and optionally a typed, not-yet-stored key) reaches the sidecar's `model.discover` method, which builds a transient provider for one listing call and never joins the configured generation. The dialect resolves from the draft's explicit `api`, else the named route's profile, else the route-id table above; an OpenAI-compatible interrogation without any endpoint fails with `CONFIG` before any network I/O. The one-shot key crosses this call only and is held by nothing afterwards.

## Request assembly

The adapter translates the assembled harness request into the sidecar's `Message`/`StreamEvent` JSON: system prompt first, then history; tool schemas travel as `name`/`description`/`input_schema`; `temperature`, `max_tokens`, and `stop` pass through. Image blocks resolve their stored bytes through `ctx.attachments` into base64 parts before assembly, including images nested inside tool-result content. Accumulated base64 image payload above the route's `maxRequestImageBytes` rejects the request with `UNSUPPORTED_CONTENT` before any network I/O — the bound is a hard cap, not a placeholder substitution. Text-only and unlisted models reject image input before credential resolution or attachment reads.

Reasoning efforts resolve per request: an omitted effort fills from the profile's `reasoningEffort` (`high` by default), so a deployment's thinking posture holds without every caller naming it. The harness vocabulary maps onto the sidecar's three levels — `low` to `low`, `high` and `max` to `high` — and an explicit `off` omits the wire field entirely, leaving the endpoint's own default in charge. The route publishes its selectable efforts through `ctx.llm.resolveModelInfo` for selector surfaces.

`streamIdleTimeoutMs` bounds each outstanding read, including the sidecar's upstream connect, without counting consumer time between chunks; expiry throws `LlmError('TIMEOUT')` and a caller abort throws `LlmError('ABORTED')`. Separately, each JSON-RPC call carries a fixed 120-second ceiling so a wedged child cannot hold a request forever. The adapter registers the configured retry policy as provider metadata; `dsh-llm-retry` executes it at durable agent-step boundaries.

## Dynamic configuration (settings + credentials)

Connection facts are not frozen at load. `resolveAdapterOptions` is the one explicit resolve step from raw config to validated routes, called at plugin load (fail loud) and once per settings snapshot at its first use. Three optional seams feed each operation:

- **`ctx.settings`** — the plugin registers the `llm-ai-sdk` namespace with this same `Config` schema and its `cordis.yml` entry as the composition base, so an `llm-ai-sdk:` user-settings section overrides any field without a restart; each route's slice lives at `providers.<route>`. A snapshot that passes the schema but fails a beyond-schema bound keeps the last good facts and logs the failure; the composition entry itself still fails plugin load. Route membership follows the live resolution and is replaced atomically, so observers never see an empty route set.
- **`ctx.credentials`** — the route's key resolves per stream call from the same snapshot that supplies the endpoint, so a rotated credential reaches the very next request. Configuration carries only `apiKeyEnv`, never a literal key. A request with no key anywhere fails with `MISSING_CREDENTIAL` naming every configuration entry point, while the route stays registered and browsable.
- **`ctx.attachments`** — resolved at request time; absence rejects image input with `UNSUPPORTED_CONTENT`.

When the resolved credential/endpoint set changes, the adapter pushes one `configure` generation to the child before the next request. Sibling routes join the generation when their keys resolve without throwing; a sibling without credentials stays unconfigured until a request names it. Every request also declares its route in the configurable-provider directory (`ctx.llm.listConfigurableProviders()`): provider `<route>`, settings namespace `llm-ai-sdk`, settings path `providers.<route>`.

## Sidecar lifecycle

The child spawns lazily on first use and initializes once; concurrent callers share one startup. Streams multiplex by `stream_id`: `chat.stream` accepts a request, `chat/event` notifications carry flat events, and `chat/done` terminates the stream. Breaking out of the iterator sends `stream.cancel` so the sidecar aborts its upstream HTTP request promptly. Child exit fails every outstanding request and stream immediately. A failed initialize (request ceiling, JSON-RPC error) tears its own generation down — killing a healthy child rather than stacking a successor beside it — and disposal (through a Cordis effect) kills the child and rejects everything still outstanding; teardown is generation-aware, so a dying child's exit event cannot tear down a successor spawned after it.

## Errors

Thrown `LlmError` codes: `NO_ADAPTER` (unknown route), `CONFIG` (no sidecar binary configured), `MISSING_CREDENTIAL`, `INVALID_CREDENTIAL` (unusable key material), `UNSUPPORTED_CONTENT` (image on a text-only model, missing attachment service, or over-budget payload), `TIMEOUT`, and `ABORTED`. Sidecar failures carried in terminal `finish` chunks map typed error kinds onto the harness vocabulary: `rate_limit` to `RATE_LIMIT`, `authentication` to `INVALID_CREDENTIAL`, `configuration` to `CONFIG`, `timeout` to `TIMEOUT`, `cancelled` to `ABORTED`, `network` and `serialization` to `TRANSPORT`, and anything else to `PROVIDER`.

## Model Experience

### Every configured route (`providers.<route>`)

#### What the model sees

The selected model receives the harness system prompt, message history, tool schemas (`name`/`description`/`input_schema`), stop sequences, and call config (`temperature`, `max_tokens`, `stop`) translated into the sidecar's message format, without adapter-authored prompt prose. Image-capable catalog entries additionally receive retained user and tool-result images as base64 parts in original order. Prior assistant reasoning travels back as ordinary assistant text, so a model that benefits from seeing its earlier reasoning still receives it.

#### Token effect

Provider tokenization governs exact text and image-token input; the sidecar reports usage counts, and cached-input tokens are separated out of the reported input total rather than double-counted. Dropping over-budget requests before sending avoids paying rejected-payload tokens; reasoning deltas become logged reasoning blocks whose retention decisions belong to the loop.

#### KV Cache effect

An unchanged assembled prefix is eligible for provider cache reuse where the endpoint offers it, reported through the usage chunk's cached-input count. A route, model, or upstream prompt/history change may prevent reuse from the first changed token; because prior reasoning returns as plain assistant text, a provider that keyed its cache on native reasoning fields may miss on those turns.

## Known Limitations and Deferred Work

- Prior-turn reasoning serializes as plain assistant text, losing any provider-native thinking signature; endpoints that require the original signature block for replayed thinking cannot recover it from this translation.
- The sidecar protocol carries three reasoning levels (`low`, `medium`, `high`); a requested `max` reaches the provider as `high` while the harness log retains `max`.
- An explicit effort of `off` omits the wire field instead of forcing thinking off, so an endpoint whose default enables thinking may still think on that request.
- Requests carry no app-attribution HTTP headers; header ownership lives inside the sidecar, which sends none today.
- Usage reporting exposes cached input tokens only; there is no cache-write metric on this path.
- A mid-stream `error` event surfaces as visible reasoning text rather than a failed finish, since it carries no failure classification.
