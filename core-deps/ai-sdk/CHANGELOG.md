# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

Native automation plugins — 2026-08-10 (`ai-computer` crate):

- **Browser plugin** (`omnichrome.rs`): `OmniChromeClient` speaks the
  OmniChrome Chrome-extension bridge protocol (JSON-RPC over
  `http://localhost:8765/rpc`, bearer token from `OMNICHROME_TOKEN` /
  `server/.bridge-token`) — navigate, click (xy/selector, with
  client-side validation preventing the bridge's 30-second-hang trap),
  human-cadence typing, screenshots (data-URL stripped → decoded PNG),
  Markdown/scrape/a11y-tree extraction, JS evaluate, raw CDP calls,
  network/console logs. Real `BrowserTool` replaces the simulated one.
- **Desktop plugin** (`native.rs`): `NativeComputerClient` +
  `ComputerTool` drive the Native Computer Use engine
  (`http://localhost:8888/rpc`, bearer from `COMPUTERUSE_TOKEN` /
  `%USERPROFILE%\.computeruse\auth.token`) — screenshots, OCR
  text-finding, Set-of-Marks UI tree, Bézier mouse/keyboard/paste,
  visual waits, window management, telemetry; keyboard actions
  enforce the engine's `target` rule client-side and surface
  sentinels like `TARGET_REQUIRED` verbatim.
- Shared authenticated JSON-RPC transport (`jsonrpc_client.rs`)
  encoding both engines' quirks: body-level errors regardless of HTTP
  status, `id:null` correlation gap tolerated via per-call awaits,
  non-standard `agent` echo ignored, data-URL handling.
- 27 offline wire proofs against handcrafted HTTP/1.1 mock engines:
  exact method/param casing, exact-string bearer auth, pre-network
  validation (zero-dial guarantees), error-code mapping
  (401/-32001→Unauthorized, -32000/-32601→typed), PNG magic checks.
- Live smoke path documented: start the engines, then tools execute for
  real; engine-down is a typed actionable error — nothing fabricated.

Live-provider hardening — 2026-08-10 (verified against
`https://inference-api.nousresearch.com/v1` with `stealth/ox-alpha`):

- `ai-providers`: reasoning surfaced from OpenRouter/Nous-style
  responses — `reasoning` string and structured `reasoning_details`
  are now accepted alongside DeepSeek's `reasoning_content`, in both
  non-streaming and streaming paths
- `ai-providers`: sampling floats (`temperature`/`top_p`/penalties)
  serialize without f32→f64 widening artifacts (`0.20000000298023224`)
  that strict gateways reject with HTTP 400; shortest-roundtrip form
  used instead (`0.2`)
- `ai-providers`: optional `AI_SDK_DEBUG_WIRE=1` writes the exact
  outbound request body to the temp dir for provider debugging
- `ai-orchestra`: planner JSON Schemas are rewritten to OpenAI
  *strict*-compatible shape (all properties required,
  `additionalProperties: false`) before being advertised via
  `ResponseFormat::JsonSchema`
- `ai-orchestra`: live end-to-end proof (credential-gated):
  real prompt → ambiguity assessment → LLM decomposition into a
  5-node task tree → 3 concurrent pooled agents → all leaves
  completed in ~37 s with full audit trail
- `ai-config`: new env vars `AI_SDK_PROVIDER` (default provider),
  `AI_SDK_PRIMARY_MODEL_CONTEXT_LENGTH` (surfaced for prompt
  budgeting); `AI_SDK_PRIMARY_MODEL` also becomes the gateway
  provider's default model; `.env.example` rewritten around them

Rust workspace scaffold and proof-driven arcs — 2026-08-10 (each with
CI-verifiable evidence):

- **AEGIS** (`ai-runtime`, `ai-core`): `ResilientModel`/`FallbackModel`
  decorators, `ResiliencePolicy` builder, deterministic fault-injecting
  chaos HTTP server; SLO proof — 200/200 (100%) successful calls under
  ~31% mixed faults (drops/stalls/500s/429s/garbage), p95 188 ms;
  breaker opens/half-opens/recovers under chaos; limiter caps
  server-observed in-flight exactly (`target/aegis-report.json`)
- **CHRONO** (`ai-observability`, `ai-agents`, `ai-devtools`, `ai-cli`):
  one correlated trace per agent run (span tree, RFC-3339 timestamps),
  error-propagating JSONL export the SDK itself writes and reloads
  losslessly, honest panic semantics; `ai trace --tui` time-travel
  explorer plus `trace diff` / `trace verify`
- **HERCULES** (`ai-agents`): per-task agent isolation
  (`Agent::derive`, ephemeral run-scoped memory by default),
  `SwarmEngine` with fan-out (real partial-failure accounting),
  hierarchical map-reduce, competitive rounds with judge ledger,
  token budgets; zero-cross-talk proof at 64 concurrent inputs;
  env-gated 1,000-task live bench writing
  `target/hercules-report.json`; fixed cumulative usage accounting,
  memory-preserving retries, unknown-tool recovery feedback, and
  HITL escalation to `AgentState::AwaitingInput`
- **LEDGER** (`ai-stream`, `ai-runtime`, `ai-devtools`, CI):
  criterion benches (SSE parse ~160 MiB/s small-event /
  ~983 MiB/s multiline), doc-truth regression linter scanning all
  crate sources for known false claims, mutation-testing pilot on
  `ai-stream` (83.3% strict kill rate, survivors documented in
  `MUTATION.md`), `bench-smoke` CI job
- **SIREN** (`ai-protocols`, `ai-voice`): `RealtimeConnection`
  WebSocket transport with tolerant unknown-event decoding,
  `DuplexSession` barge-in loop measured at 29 µs detection→cancel
  (bound 300 ms), real WAV parser with proptest round-trip,
  adaptive noise-floor VAD (0 false triggers on ramped-noise
  fixture vs ≥5 for the fixed threshold), configurable STT/TTS
  builders
- **MINERVA** (`ai-memory`, `ai-rag`, `ai-cache`): char-ngram
  embeddings raising eval recall@5 from 0.857 to 1.000 (ties reported
  honestly), true reciprocal-rank fusion + corpus-statistics BM25
  (pipeline recall cap removed: 24→30 of 31 fixture hits end-to-end),
  `CachedModel` request cache wired through the model seam with
  hit/miss counter proofs

### Fixed

Truth-sprint hardening — 2026-08-10:

- `ai-rag`: chunking forward-progress fix — overlap handling vs UTF-8
  boundary snapping could stop advancing and loop/OOM on crafted input
- `ai-rag`: proptest redesigns covering those chunker edge cases
- `ai-providers`: streamed tool-call argument assembly now tracks by
  tool-call id instead of chunk index; `Retry-After` response header
  is honored on retries
- `ai-runtime`: circuit-breaker probe-permit leak fixed (permit no
  longer lost on early exit); limiter cold-start off-by-one fixed;
  retry backoff switched to decorrelated jitter
- `ai-stream`: property-test harness aligned with the SSE serializer
  (round-trip parity)
- `.github/workflows/ci.yml`: live-gateway job gate fixed — a job-level
  `if` can access neither the `secrets` nor the `env` context (only
  `github`/`inputs`/`needs`/`vars`; gating on `env.… != ''` at job level
  made the whole workflow invalid). Credentials are exposed at
  workflow-level `env` and the gate is applied per step via
  `if: env.AI_SDK_GATEWAY_API_KEY != ''`, which is valid where `env`
  exists; without secrets the job is a green no-op

### Added

- Rust workspace scaffold (25 crates) per `ENGINEERING-SPEC.md` — 2026-08-09
- `ENGINEERING-SPEC.md`: Rust engineering specification adapted from the
  master template to the PRD v1.2 vision
- `ENGINEERING-LOG.md`: chronological execution report (what/assumed/passed/
  failed/root-cause/fix) for every stage, including live gateway testing
- `ai-types`, `ai-errors`, `ai-config`, `ai-models`, `ai-core`: domain types,
  typed error hierarchy, unified configuration, model registry with pricing,
  core traits + `AiClient` (26 unit tests)
- `ai-stream`: SSE parser (CRLF/CR/LF, multiline data, chunk boundaries,
  EOF flush, multi-event chunks), `collect_text`/`collect_completion`,
  `ToolCallAccumulator` with finalization (12 unit tests)
- `ai-runtime`: retry with exponential backoff+jitter, per-key concurrency
  limits, parallel fan-out/fan-in with deadlines and partial results,
  race/fallback, circuit breaker (22 unit tests)
- `ai-providers`: real OpenAI-compatible adapter (OpenAI wire protocol with
  DeepSeek extras: reasoning deltas, cache-aware usage) — works against the
  project gateway `opencode.ai/zen/go/v1` (16 unit tests)
- Live integration tests (`crates/ai-providers/tests/live_gateway.rs`):
  13 real-API tests covering discovery, generate, streaming, tool loop,
  structured output, vision, auth/unknown-model errors, and parallel calls
  across both models — **all 13 pass against the real gateway**
- ADR-011: Rust implementation language (supersedes ADR-001)
- ADR-012: workspace restructuring (removed dead `sdk` submodule gitlink)
- `.env.example` credential template; MIT license

### Added

- **Fully self-hosted**: removed the external Firecrawl REST adapter and
  `FIRECRAWL_API_KEY`; web research is now 100% native + LLM-driven
  extraction
- `StatisticalEmbeddings`: real local feature-hashing embeddings (FNV-1a,
  log term-frequency, L2 normalization) for semantic memory and RAG with
  zero external services
- `ChatRequest.top_p` / `frequency_penalty` / `presence_penalty` wired
  through the OpenAI-compatible adapter
- Live self-hosted tests: RAG retrieval → grounded answer
  ("Mimo-v2.5"), semantic-memory ranking — both with `deepseek-v4-flash`
  only (16/16 live suite)

### Changed

- Web research layer documented as self-hosted (no external scraping API)

### Added

- **MCP dual-era**: `McpServer::enable_legacy()` serves legacy
  initialize-handshake clients (2025-11-25) alongside modern stateless
  clients; `McpClient::with_legacy()` performs the real handshake
- **Native Anthropic adapter** (`anthropic.rs`): Messages API with
  x-api-key/anthropic-version headers, tool use blocks, SSE streaming
- **Native Google Gemini adapter** (`gemini.rs`): generateContent with
  function declarations, inline images, SSE streaming
- **Fine-tuning jobs API client** (`finetune.rs`): create/list/get/cancel
  jobs, events, training-file upload (real OpenAI wire format)
- `StructuredExtractor` + `LlmStructuredExtractor`: real schema-driven
  extraction for `NativeResearchBackend::extract` (fail-fast without an
  extractor — no more partial output)
- GitHub Actions CI: fmt, clippy -D warnings, workspace tests, and a
  credential-gated live-gateway job

### Changed

- `create_provider` routes `anthropic`/`google` to native adapters
- README status badge updated to Verified

 `ai-memory` (four-tier + compaction),
  `ai-rag` (chunking/hybrid retrieval/reranking), `ai-workflows` (DAG
  engine with checkpoints), `ai-agents` (agent runtime, sub-agents,
  swarms, HITL, self-healing retries), `ai-analytics`, `ai-devtools`,
  `ai-edge`, `ai-voice` (VAD, real STT/TTS adapters)
- `ai-cli` binary: `doctor`, `providers`, `models`, `config`, `run`,
  `inspect`, `trace`, `benchmark` (all real, verified live)
- `ai-sdk` facade crate with `prelude`
- Live examples: `chat`, `agent`, `parallel`
- Live agent tool-loop test (`deepseek-v4-flash`): agent drives a real
  calculator call to "42"

### Fixed

- `ai-config` now reads the gateway env vars
  (`AI_SDK_GATEWAY_BASE_URL`/`AI_SDK_GATEWAY_API_KEY`) into the
  `opencode` provider (found by the CLI doctor smoke test)
- `ai-workflows` recursive executor rewritten to a boxed, state-by-value
  design (Send-provable; correct retry semantics)
- `ai-runtime::retry` accepts `FnMut` closures (needed for state capture)
- Model catalog pricing was per-1M while documented per-1K (1000× cost
  error) — corrected to per-1K semantics
- SSE parser treated `\r\n` as two line terminators (multiline `data:`
  broken) — CRLF consumed as one terminator
- Parallel executor dropped concurrency permits immediately (limits silently
  ignored) — permits held for task duration
- Parallel executor discarded completed results when a deadline fired —
  shared result store preserves partial results
- **SSE parser dropped all events after the first in multi-event network
  chunks** (found by live gateway testing; raw-probe verified the gateway
  stream was complete) — parser now consumes whole chunks before yielding

### Changed

- **MCP rewritten to the modern stateless 2026-07-28 revision** (ADR-013):
  per-request `_meta` protocol version + capabilities (no `initialize`
  handshake), `server/discover`, `resultType`, MRTR with elicitation,
  `subscriptions/listen` with `subscriptionId`, error codes
  `-32020/-32021/-32022`, Streamable HTTP transport with
  `MCP-Protocol-Version` header and HTTP 400 mapping. Legacy
  initialize-handshake era not implemented (documented; dual-era = roadmap).
- Repository restructured from docs-only to a Cargo workspace at repo root.
- README rewritten for the Rust implementation.

### Removed

- Dead `sdk/` submodule gitlink (no `.gitmodules` URL existed).
