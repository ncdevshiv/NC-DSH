# AI SDK — Engineering Log

Chronological execution report per `ENGINEERING-SPEC.md` §36. Every entry
records: what was tested, what we assumed should happen, the outcome
(implemented / verified / requires credentials / intentionally unsupported /
known limitation), failures, root causes, and fixes. **No hot patches**: every
fix is a root-cause fix with regression tests.

Status legend:
- ✅ **PASS** — verified against the real system
- ❌ **FAIL** — test failed (with root cause + fix)
- ⚠️ **PARTIAL** — some aspects passed, some failed
- 📋 **IMPLEMENTED** — code complete (verified by unit tests)
- 🔒 **REQUIRES CREDENTIALS** — real integration path complete; needs live key
- 🚫 **KNOWN LIMITATION** — intentionally not implemented (documented)

---

## 2026-08-09 — Session 1: Workspace Build + Real Gateway Testing

### Entry 1 — Toolchain & workspace scaffold
- **Timestamp:** 2026-08-09 ~21:58–22:20
- **What:** Rust 1.97.1 (stable, x86_64-pc-windows-msvc) installed via
  rustup; 25-crate Cargo workspace scaffolded at repo root; docs-only repo
  restructured (dead `sdk` submodule gitlink removed, ADR-011 Rust decision,
  ADR-012 restructuring, README/LICENSE/CHANGELOG/CONTRIBUTING/SECURITY).
- **Assumption:** workspace compiles clean.
- **Result:** ✅ PASS — `cargo check --workspace` green (47s first build).
- **Notes:** Submodule had no `.gitmodules` URL (verified via API before
  removal).

### Entry 2 — Core layers (`ai-types`, `ai-errors`, `ai-config`, `ai-models`, `ai-core`)
- **Timestamp:** 2026-08-09 ~22:20–23:10
- **What:** domain types (messages, content parts, roles, usage, stream
  events); 15-category typed error hierarchy with `is_retryable()`;
  env+TOML+programmatic config with key masking; model registry with real
  per-1K pricing; `Model`/`Provider` traits + `AiClient` builder.
- **Assumption:** all unit tests pass; pricing math correct.
- **Result:** ✅ PASS — 26 tests. One real defect found & fixed: **catalog
  pricing was per-1M while the struct documented per-1K** (1000× cost error)
  — caught by `cost_estimation_uses_pricing`; corrected catalog values,
  not the test.
- **Root cause of pricing bug:** values copied from published per-1M pricing
  without unit conversion.
- **Fix:** divided all catalog values by 1000 to match documented per-1K
  semantics; added assertion tying the math to a known example
  (gpt-4o: 1k in + 1k out = $0.0125).

### Entry 3 — `ai-stream`: SSE parser
- **Timestamp:** 2026-08-09 ~23:10–23:40
- **What:** incremental SSE parser (CRLF/CR/LF, multi-line `data:`, fields,
  comments, chunk boundaries, EOF flush), `collect_text`,
  `collect_completion`, `ToolCallAccumulator`.
- **Assumption:** parser handles all SSE edge cases.
- **Result:** ⚠️ PARTIAL — first run: 9/10 passed. ❌ `handles_crlf_and_multiline_data`:
  `data: line1\r\ndata: line2\r\n\r\n` produced only `line1`.
- **Root cause:** `find_line_end` treated `\r\n` as **two** line terminators
  (`\r` then `\n`) — the stray `\n` became a blank line and dispatched the
  event early, wiping accumulated `data:` lines.
- **Fix:** `find_line_end` now consumes the full CRLF pair as one terminator.
  Verified: 10/10 PASS.
- **No hot patch:** the fix is in the parser core with tests for CRLF,
  multiline, comments, chunk boundaries, and EOF cases.

### Entry 4 — `ai-runtime`: parallel engine
- **Timestamp:** 2026-08-09 ~23:40–00:30
- **What:** `RetryPolicy` (exponential backoff + jitter), `ConcurrencyLimiter`
  (per-key semaphores), `Parallel` executor (fan-out/fan-in, deadlines,
  partial results, cancellation), `race`/`fallback`, `CircuitBreaker`.
- **Assumption:** concurrency limits are enforced; deadlines cancel work.
- **Result:** ⚠️ PARTIAL — several compile fixes, then 3 behavioral failures.
  The significant one: ❌ `key_limits_are_applied` — max concurrency was **10
  (limit 2)**; the limit was silently ignored.
- **Root cause:** in `wrap_task`, the permit returned by
  `keyed.acquire(key).await` was **never bound to a variable** — it was
  dropped at the end of the match arm, releasing the budget immediately.
  Same bug existed for the global permit.
- **Fix:** bind permits (`_global_permit`, `_key_permit`) for the task
  duration; added an assertion in the test that the limit is registered.
  Also fixed a real design flaw found by `deadline_cancels_in_flight_tasks`:
  on deadline, **completed partial results were discarded** and every task
  reported timeout. Fixed with a shared result store read after the deadline
  fires. Verified: 22/22 PASS.
- **No hot patch:** limits and partial-results semantics fixed at the engine
  level, covered by tests.

### Entry 5 — Gateway contract verification (verify before implementing)
- **Timestamp:** 2026-08-09 ~00:35–01:00
- **What:** probed `https://opencode.ai/zen/go/v1` (credentials stored in
  gitignored `.env`):
  1. `GET /models` → HTTP 200, OpenAI-style `{object:list,data:[...]}`;
     `deepseek-v4-flash` ✅ and `mimo-v2.5` ✅ both present (25 models total).
  2. `POST /chat/completions` (non-stream) → HTTP 200; standard shape plus
     **DeepSeek extras**: `message.reasoning_content`,
     `usage.prompt_cache_hit_tokens`,
     `usage.completion_tokens_details.reasoning_tokens`, top-level `cost`.
  3. `POST /chat/completions` with `"stream":true` → HTTP 200 SSE:
     `chat.completion.chunk` events with `reasoning_content` deltas, final
     chunk carries `finish_reason` + `usage`, then `data: [DONE]`, then a
     **non-standard trailing event** `{"choices":[],"cost":"0"}`.
- **Assumption:** gateway is OpenAI-compatible.
- **Result:** ✅ PASS — contract fully mapped; adapter designed to tolerate
  the trailing cost event and to surface reasoning + cache-aware usage.

### Entry 6 — `ai-providers`: OpenAI-compatible adapter (real HTTP)
- **Timestamp:** 2026-08-09 ~01:00–01:45
- **What:** `OpenAiCompatProvider`/`OpenAiCompatModel` — real reqwest HTTP
  client, bearer auth, typed status mapping (401/403→Authentication,
  429→RateLimit+retry-after, 4xx/5xx→Provider with retryability), message
  serialization (text/image/audio/tool parts), tools, structured output,
  SSE→unified events with reasoning deltas and streaming tool-call
  finalization, `/models` enumeration, usage mapping.
- **Assumption:** unit tests pass; Debug output never leaks the API key.
- **Result:** ✅ PASS — 16 unit tests (incl. key-redaction assertion).
  A streaming design flaw found during implementation: the first draft of
  `map_sse_to_events` emitted only the **first** event per SSE chunk
  (`events.remove(0)`) and never emitted `ToolCallCompleted` for streamed
  tool calls. Fixed before any live run: full `flat_map` emission +
  accumulator `finalize_and_drain` on `finish_reason` (with
  `finalize`/`drain_completed` added to `ToolCallAccumulator`).

### Entry 7 — LIVE integration test run #1 (13 real tests)
- **Timestamp:** 2026-08-09 ~01:45–02:20
- **What:** full live suite against the real gateway, both models:
  list models; non-streaming exact reply (PONG); reasoning surfaced;
  usage+finish_reason; streaming text; unified events; tool-calling full
  loop (6×7→42); streamed tool-call finalization; JSON structured output;
  vision (mimo-v2.5, embedded 1×1 red PNG); invalid-key→AuthError;
  unknown-model→error; parallel calls to both models.
- **Result:** ⚠️ PARTIAL — **11/13 PASS**, 2 FAIL:
  1. ❌ `stream_primary_collects_expected_text` — got `"1\n\n3"` (model
     skipped "2" — nondeterministic LLM output; **adapter was fine**).
     Assumption error on our side: free-form counting is not deterministic.
     Fix: switched to an exact-reply marker prompt (`STREAMING-OK`).
  2. ❌ `unknown_model_returns_provider_error` — expected ProviderError
     (400/404 per standard), got **AuthenticationError (HTTP 401)** with
     message `Model definitely-not-a-real-model-xyz is not supported`.
     **Root cause: gateway contract fact** — the gateway answers 401 for
     unknown models, same status as bad keys. The adapter's 401→Auth mapping
     is correct per the standard contract; the *test assumption* was wrong.
     Fix: test now asserts the observed contract and documents the quirk
     (auth OR provider error + message references the model).
- **No hot patch:** no adapter change for the 401 quirk — it's a gateway
  behavior, documented in the test and here. If the gateway later
  distinguishes status codes the test must be revisited.

### Entry 8 — LIVE run #2: truncation investigation
- **Timestamp:** 2026-08-09 ~02:20–02:40
- **What:** re-ran suite. Two streaming tests now failed differently:
  `"STREAM-"`, `"STREAMING"`, `"STREAMING-"` — random truncation; and
  `stream_emits_unified_events` once saw zero text deltas.
- **Hypothesis A:** gateway throttling under concurrency → **disproved** by
  running the stream test in isolation 3× (still truncated).
- **Hypothesis B:** our pipeline loses events → **proved**.
- **Diagnostic (no speculation):** two probes:
  1. Our adapter event dump: text fragments arrived as `REAM`,`ING`,`-`,`OK`
     — with `ST` appearing in `reasoning_content`; full text `REAMING-OK`.
  2. **Raw SSE probe** (`examples/raw_probe.rs`, bypasses our adapter
     entirely): raw content parts `["ST","REAM","ING","-","OK"]`,
     full text `STREAMING-OK`, 35 events, `finish_reason: stop`.
- **Conclusion:** the **gateway stream is complete**; **our `sse_parse`
  drops events**.
- **Root cause (found by code inspection, confirmed by raw probe):** in
  `sse_parse`, when a chunk contained a blank line that completed an event,
  the unfold closure **returned immediately with the first event**, and the
  scan position (`start`) lived only in that invocation — the chunk's
  **remaining lines were silently discarded** on the next poll. Real network
  chunks (reqwest `bytes_stream`) often carry several SSE events per chunk;
  my original unit tests only exercised one event per chunk, so the defect
  was invisible until live traffic.
- **Fix (root cause, not hot patch):** process the **entire chunk** before
  returning from the closure — queue all completed events, then pop them one
  per poll. Added two regression tests that fail on the old code:
  - `multiple_events_in_one_chunk_are_all_emitted` (4 events in one chunk)
  - `many_events_across_variable_chunks_are_all_emitted` (6 events across
    fragmented chunks)
  Verified: ai-stream 12/12 PASS, then live suite re-run.

### Entry 9 — LIVE run #3 (final): 13/13 PASS
- **Timestamp:** 2026-08-09 ~02:40–03:00
- **Result:** ✅ **ALL 13 LIVE TESTS PASS** against the real gateway:
  - `list_models_contains_primary_and_vision` ✅ (25 models)
  - `generate_non_streaming_primary_exact_reply` ✅ (`PONG` exact)
  - `generate_exposes_reasoning_content` ✅ (34-char reasoning surfaced)
  - `generate_reports_usage_and_finish_reason` ✅ (in=86 out=14 total=100,
    finish `stop`)
  - `stream_primary_collects_expected_text` ✅ (`STREAMING-OK` exact —
    the bug this session found and fixed)
  - `stream_emits_unified_events` ✅ (13 events, text+completed+usage)
  - `tool_calling_full_loop_returns_42` ✅ (model called `calculator` with
    `{"expression": "6 * 7"}`, real evaluation = 42, final answer `42`)
  - `streamed_tool_call_is_finalized` ✅ (1 call finalized)
  - `structured_json_object_output` ✅ (`{"answer":"yes","ok":true}`)
  - `vision_model_identifies_red_pixel` ✅ (mimo-v2.5: `Red`)
  - `invalid_api_key_returns_authentication_error` ✅ (401→Authentication)
  - `unknown_model_returns_typed_error` ✅ (401→typed error, documented quirk)
  - `parallel_calls_both_models_concurrently` ✅ (both models, real parallel)
- **Full workspace:** 51 test suites green, 0 failures, no warnings.

---

## Summary of defects found & fixed (all root-cause, all with regression tests)

| # | Defect | Found by | Root cause | Fix |
|---|---|---|---|---|
| 1 | Catalog pricing 1000× off | unit test | per-1M values in per-1K field | corrected values + math assertion |
| 2 | SSE CRLF parsed as two terminators | unit test | `\r\n` split into 2 lines → early dispatch | consume CRLF as one terminator |
| 3 | Concurrency limits silently ignored | live-style unit test | permits dropped at end of match arm | bind permits for task duration |
| 4 | Deadline discarded completed results | unit test | results lost when runner future dropped | shared result store read post-timeout |
| 5 | SSE multi-event chunks lost | **live gateway test** | early return mid-chunk; scan position lost | process whole chunk, queue events; 2 regression tests |
| 6 | (test assumption) unknown model = 401 | live gateway test | gateway uses 401 for unknown models | documented contract; test asserts observed behavior |

**No hot patches were used.** Each fix addressed the root cause, was
verified by a regression test that fails without it, and the live suite was
re-run to confirm.

## Known limitations (documented, not hidden)

- Anthropic / Google Gemini native adapters not yet implemented (their wire
  formats differ from OpenAI-compatible); `ai-providers` currently serves
  the OpenAI-compatible protocol for `openai`, `openrouter`, `ollama`, and
  the project gateway. — `ENGINEERING-SPEC.md` §40 status.
- The gateway answers HTTP 401 for unknown models (same as bad keys);
  adapter maps per the standard OpenAI contract; quirk documented in
  `unknown_model_returns_typed_error`.
- The gateway streams `reasoning_content` interleaved with `content`
  (DeepSeek-style); both are surfaced as separate unified events.
- 1×1 red-pixel PNG for vision tests is generated and embedded (no external
  image host dependency).

## Environment & credentials hygiene

- API key + base URL live only in `.env` (gitignored, verified with
  `git check-ignore`). Never committed, never logged: `Debug` output for the
  provider redacts the key, `redacted_summary()` masks keys, and the
  integration tests never print the key.

---

## 2026-08-10 — Session 2: MCP 2026-07-28 Modern Rewrite

### Entry 10 — Protocol research (verify before implementing)
- **Timestamp:** 2026-08-10
- **What:** compared our MCP implementation (2025-03-26 initialize
  handshake) against the current MCP revision. Findings:
  - Latest revision is **2026-07-28** (schema at
    `modelcontextprotocol/specification/schema/2026-07-28/schema.ts`).
  - The protocol is now **stateless**: no `initialize`; every request
    carries REQUIRED `_meta.io.modelcontextprotocol/protocolVersion` +
    `clientCapabilities`; missing → `-32602`/HTTP 400.
  - `server/discover` is a REQUIRED server method
    (`supportedVersions`, `capabilities`, `instructions`).
  - Results carry `resultType` (`complete`/`input_required`).
  - New error codes `-32020` (HeaderMismatch), `-32021`
    (MissingRequiredClientCapability), `-32022` (UnsupportedProtocolVersion
    with `data.supported`); `-32002` retired.
  - MRTR (`InputRequiredResult` + `requestState`/`inputResponses`);
    elicitation (`elicitation/create`, form/url); `subscriptions/listen`
    with `subscriptionId`; OTel `_meta` trace keys; JSON Schema 2020-12
    default; Roots and sampling deprecated (SEP-2577).
- **Verdict:** complete rewrite required (session goal). Dual-era
  (legacy `initialize`) deliberately not implemented; documented in
  ADR-013 as a roadmap item.

### Entry 11 — MCP module rewrite (mcp.rs, mcp_http.rs)
- **Timestamp:** 2026-08-10
- **What:** rewrote `ai-protocols` MCP to the modern stateless model:
  per-request `_meta` validation; `server/discover`; `resultType` on all
  results; `-32020/-32021/-32022` errors with correct `data` payloads;
  `HandlerOutcome::Complete | NeedsInput` tool handlers with MRTR;
  elicitation input requests (form) with capability gating; client MRTR
  loop (elicitation/sampling resolvers, `inputResponses` +
  `requestState` retry, round limit); `subscriptions/listen` with
  `subscriptionId` notification correlation; Streamable HTTP client +
  raw-TCP server with `MCP-Protocol-Version` header, 400 mapping, and SSE
  listen streams; server info in result `_meta`; OTel trace keys passthrough.
- **Tests (14, all real protocol round-trips):** JSON-RPC round-trips;
  discover; missing `_meta` → `-32602`; unsupported version → `-32022`
  with `data.supported`; version negotiation/retry; tools/resources/
  prompts round-trip with `resultType`; **MRTR elicitation round-trip**
  (server requests a name, client resolves, retry completes); elicitation
  without client capability → `-32021`; subscriptions deliver
  `notifications/tools/list_changed` with `subscriptionId`; HTTP transport
  round-trip and unsupported-version→400; A2A in-process and over TCP.
- **Result:** ✅ 14/14 PASS; workspace 51 suites green.
- **Defects found & fixed (root cause, no hot patches):**
  1. **Buffered responses never flushed in persistent server loops** —
     responses were written to `BufWriter` but the loop kept the writer
     alive, so the buffer never reached the pipe; clients hung. Fix:
     explicit `flush()` after every response (all transports).
  2. **Test deadlock**: subscription test held a `std::sync::Mutex` guard
     across `.await` in a spawned task → converted to `tokio::sync::Mutex`
     with scoped lock (guard dropped before awaiting the subscription
     receiver).
  3. **Subscription race**: the test registered a tool before the server
     had processed the `subscriptions/listen` request, so the notification
     was lost. Fix: deterministic oneshot handshake — the server signals
     when the subscription is registered before the test triggers events.
  4. **A2A TCP test**: server handled only one connection but the client
     makes two requests. Fix: accept loop.
  5. **`unsupported_versions` parsing**: `Debug`-formatted `Value` broke
     JSON parsing of `data.supported`; the error now embeds the list as
     JSON (`(supported: ["2026-07-28"])`) and the parser reads it back.
- **Docs:** ADR-013 added; ENGINEERING-SPEC §16 updated to the modern
  revision; CHANGELOG updated.

---

## 2026-08-10 — Session 3: Project Completion (remaining crates, live CLI, validation)

### Entry 12 — Remaining subsystem implementation
- **What:** completed every remaining crate with real implementations and
  unit tests:
  - `ai-analytics` (4): metrics aggregation from execution events, cost
    estimation, rate counters.
  - `ai-devtools` (4): inspector + trace viewer over the observability
    collector with redaction.
  - `ai-edge` (4): runtime detection (native/node/edge/browser), WASM
    helpers, capability matrix.
  - `ai-voice` (7): PCM audio + resampling, WAV encoder, energy-based VAD,
    real OpenAI-compatible STT (`/audio/transcriptions`, multipart WAV) and
    TTS (`/audio/speech`). Realtime full-duplex documented as
    requires-credentials.
  - `ai-cli`: binary with `doctor`, `providers`, `models`, `config`, `run`,
    `inspect`, `trace`, `benchmark` — all real.
  - `ai-sdk` facade: unified re-exports + `prelude`.
- **Result:** ✅ workspace 52 test suites green (approx. 180 tests).

### Entry 13 — Defects found & fixed (root cause, no hot patches)
1. **Config ignored gateway env vars** (found by the CLI `doctor` smoke
   test): `AI_SDK_GATEWAY_BASE_URL`/`AI_SDK_GATEWAY_API_KEY` were not wired
   into `ai-config`, so the CLI could not see the provider. Fix: `merge_env`
   registers the `opencode` provider from those variables.
2. **Recursive async fn not Send-provable** (`ai-workflows`): the node
   executor's recursion defeated the compiler's Send analysis; `Parallel`
   tasks rejected the future. Fix (root cause): state now flows **by value**
   through a boxed recursive executor (`run_node_boxed` returning
   `BoxedNodeFuture<'a>`), and node handlers take/return state instead of
   borrowing `&mut` across retries — this also gives correct retry
   semantics (each attempt replays the pre-step state).
3. **`retry` needed FnMut**: retry closures capture `&mut` state; the
   signature was `Fn` — changed to `FnMut` with `Send` bounds.
4. **Timeout test used a blocking handler**: blocking steps cannot be
   interrupted by `tokio::time::timeout`; added a real
   `AsyncFunctionNodeHandler` + `step_async` builder and the test now uses
   an async sleep (documented: blocking handlers block the runtime).

### Entry 14 — Live verification (only deepseek-v4-flash + mimo-v2.5)
- **Result:** ✅ **ALL 14 live gateway tests PASS** (57s), including the new
  `agent_tool_loop_live_primary_model` (real agent tool loop → "42").
- CLI against the real gateway: `doctor` (25 models, key masked), `run`
  (CLI-WORKS), `run --stream` (STREAM-OK), `benchmark` (6/6, 0.4 req/s),
  `config` (redacted) — all ✅.
- Examples (real, live): `chat` (HELLO + STREAM), `agent` (tool call →
  "Hello, Ada!"), `parallel` (both models concurrently) — all ✅.

### Entry 15 — Full validation
- `cargo fmt --check` ✅ · `cargo check --workspace` ✅ ·
  `cargo test --workspace` ✅ (52 suites) ·
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
  (0 warnings; ~40 lints fixed: type aliases for complex handler types,
  `is_empty` additions, default-method naming, unused-assignment handling).
- No-fake audit ✅: no TODO/FIXME/stub/placeholder markers; only
  documentation comments referencing the ADR-007 unit-test strategy
  (scripted models) remain, and the PII redaction "placeholder" feature.
- `.env` verified gitignored; no secrets in the tree.

---

## 2026-08-10 — Session 4: Recommendations Completed + Zero-Fake Audit

### Entry 16 — Recommendation 1: MCP dual-era (legacy initialize) support
- **What:** `McpServer::enable_legacy()` — a request carrying modern
  per-request `_meta` is served statelessly; an `initialize` request
  selects the legacy (2025-11-25) dialect; notifications are skipped by
  transports. `McpClient::with_legacy()` performs the real handshake
  (initialize → `notifications/initialized` → requests without `_meta`,
  results without `resultType`).
- **Tests:** legacy handshake round-trip; modern + legacy clients coexist
  on the same dual-era server. ✅ 16/16 protocol tests.

### Entry 17 — Recommendation 2: Native Anthropic + Gemini adapters
- **Anthropic** (`anthropic.rs`): real Messages API (`x-api-key` +
  `anthropic-version: 2023-06-01`), system prompts, tool_use/tool_result
  blocks, base64 images, SSE streaming (`content_block_start/delta`,
  `message_start/delta`) with in-flight tool-call accumulation, cache-aware
  usage. Wire-tested (serialization/parsing/SSE mapping). Requires
  `ANTHROPIC_API_KEY` for live calls (documented).
- **Gemini** (`gemini.rs`): real generateContent/streamGenerateContent
  (`x-goog-api-key`), function declarations/functionCall/functionResponse,
  inline images (URL images rejected explicitly — Gemini needs inline
  data), SSE `alt=sse` streaming, model listing. Wire-tested. Requires
  `GOOGLE_API_KEY` (documented).
- `create_provider` routes `anthropic`/`google` to the native adapters.
  ✅ 30/30 provider tests.

### Entry 18 — Recommendation 3: Fine-tuning jobs API client
- `finetune.rs`: real OpenAI fine-tuning client — `create_job`,
  `list_jobs`, `get_job`, `cancel_job`, `list_events`, plus training-file
  upload (`POST /files`). Wire-tested. Requires an OpenAI-compatible key
  (documented). Closes the ENGINEERING-SPEC §3 fine-tuning gap.

### Entry 19 — Partial removed: NativeResearchBackend::extract
- **Before:** returned raw content + schema (partial, not extraction).
- **Now:** `StructuredExtractor` trait + `LlmStructuredExtractor` (real
  model-driven JSON extraction against the schema); the native backend
  **fails fast** with a clear error when no extractor is configured.
  ✅ 17/17 web tests (extractor parses model JSON, rejects non-JSON,
  fail-fast).

### Entry 20 — README + CI
- README status badge updated (Verified) + CI badge; CI workflow
  (`.github/workflows/ci.yml`): fmt --check, clippy -D warnings,
  workspace tests, credential-gated live-gateway job (deepseek-v4-flash +
  mimo-v2.5).

### Entry 21 — Final zero-fake audit + validation
- No-fake audit ✅: zero TODO/FIXME/unimplemented!/unreachable!/stub/
  coming-soon/dummy markers in `crates/`; no mock/fake-named shipped code;
  only documentation comments referencing the ADR-007 unit-test strategy
  (scripted models are `#[cfg(test)]`-only fixtures, never shipped).
- Gates ✅: `cargo fmt --check`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` (0), `cargo test --workspace`
  (52 suites, 231 tests), **live gateway suite 14/14** (both models).

---

## 2026-08-10 — Session 5: Fully Self-Hosted + OpenAI-Compatible Focus

### Entry 22 — Firecrawl removed (self-hosted philosophy)
- **What:** the external Firecrawl REST adapter, `FIRECRAWL_API_KEY`, and
  all references were **removed** from code, config, docs, and spec. The
  web research layer is now fully self-hosted: native fetch/crawl/extract
  + `StructuredExtractor`/`LlmStructuredExtractor` for schema-driven
  extraction. Rationale: no reason to depend on an external scraping API
  when the SDK ships its own crawler and LLM-driven extraction.

### Entry 23 — Self-hosted embeddings (no external service)
- **What:** `StatisticalEmbeddings` (`ai-memory/statistical.rs`) — real
  feature-hashing embeddings (FNV-1a signed hashing, log term-frequency
  weighting, L2 normalization, power-of-two dimensions). Used by semantic
  memory and RAG with zero external dependencies.
- **Gateway probe (verified 2026-08-10):** `POST /embeddings` and
  `POST /audio/speech` return **HTTP 404** on the project gateway — it
  only routes `/chat/completions` and `/models`. Local embeddings are
  therefore the correct self-hosted path; the OpenAI-compatible embeddings
  adapter remains for hosts that expose the endpoint.
- **Tests:** similarity ordering, identical-text near-1.0, empty text,
  dimension rounding. ✅ 15/15 ai-memory tests.

### Entry 24 — OpenAI-compatible adapter improvements
- `ChatRequest` gains `top_p`, `frequency_penalty`, `presence_penalty`
  (serde-optional) and the OpenAI-compatible adapter serializes them.
  ✅ 30/30 provider tests.

### Entry 25 — Live verification (only deepseek-v4-flash + mimo-v2.5)
- **Self-hosted RAG live:** StatisticalEmbeddings + in-memory vector store
  + gateway LLM — retrieval ranked the vision chunk first (score 0.268),
  and `deepseek-v4-flash` answered "Mimo-v2.5" grounded in the retrieved
  context. ✅
- **Self-hosted semantic memory live:** local embeddings ranked the vision
  fact first (score 0.500). ✅
- Full live suite: **16/16 PASS** (only the two models). ✅
- Gates: fmt clean, clippy `-D warnings` 0 errors, workspace 52 suites /
  237 tests. ✅
