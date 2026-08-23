# AI SDK — Rust Engineering Specification (Adapted from Master Template)

**Status:** Active engineering goal
**Date:** 2026-08-09
**Repo:** `ncdevshiv/ai-sdk` (workspace: `F:\alisia\ai-sdk`)
**Source spec:** Generalized "Senior Rust AI SDK / ADK Engineering Specification" (adapted)
**Guiding documents:** [`PRD-v1.md`](./PRD-v1.md), `ADRs/` (10 records), [`VERIFICATION_REPORT.md`](./VERIFICATION_REPORT.md)

---

## 0. Adaptation Summary (what changed vs. the generalized template)

The generalized template was adapted to **our aim**: the comprehensive AI SDK defined in `PRD-v1.md` (multi-provider, multi-agent, MCP + A2A, memory, RAG, voice, compliance, edge, devtools, CLI, swarms, self-healing). Adaptations:

| Template item | Adaptation |
|---|---|
| Language | **Rust** (kept per template + user decision). **Deviation from ADR-001** (which selected TypeScript) is documented here and must be recorded as ADR-011. |
| Crate layout | Mapped to the PRD feature areas; `ai-stream`, `ai-protocols`, `ai-rag`, `ai-voice`, `ai-devtools` added; `ai-edge` mapped from PRD §4.4 (WASM/edge). |
| Provider tiers | PRD §2.1 Tier 1/Tier 2 provider lists adopted. |
| Protocols | Template's MCP/A2A requirements made explicit (PRD §3.1/§3.2): native MCP client + server, A2A client + server. |
| Agent system | PRD §3.3 patterns (hierarchical, pipeline, parallel, router, collaborative) + swarms (§3.10) + self-healing (§3.11) adopted. |
| Skills system | PRD §3.3 "Skills" (registry, versioning, discovery) folded into `ai-tools`. Self-skill creation (§3.13) is **deferred** (see §3 Scope). |
| Memory | PRD §3.4 four-tier model (working, short-term, long-term, semantic) adopted. |
| RAG | Made a first-class crate (`ai-rag`): chunking, hybrid search, reranking (PRD §3.8). |
| Fine-tuning / LoRA | PRD §3.6: **scoped** — real OpenAI fine-tuning job API adapter only; local LoRA training explicitly unsupported in this build (see §3). |
| Computer use / browser automation | PRD §3.7: **deferred** (see §3). Web research layer (§9/§10) *is* in scope. |
| Voice / multi-modal | PRD §3.5: `ai-voice` crate with real trait + real provider adapters; provider-dependent paths require credentials (compile + real integration path, documented). |
| Coding workflows / IDE / LSP | PRD §3.7.1–3.7.3: **deferred** (roadmap only, documented). |
| Repo restructuring | The dead `sdk` gitlink (submodule with no URL) is removed; the Rust workspace lives at repo root. |
| Hardcode Rules | The `VERIFICATION_REPORT.md` "Hardcode Rules" (no mocks/demos/fakes/stubs/TODOs) are re-affirmed verbatim in §33/§35. |

---

## 1. Core Requirement

Design and implement a **complete, production-grade AI SDK/ADK as a Rust workspace monorepo** realizing the `PRD-v1.md` vision: a unified, extensible foundation for building sophisticated AI agents and applications.

This is not a prototype, proof of concept, demo, mock implementation, educational example, or architectural sketch. The final result must be a **real, executable, production-oriented codebase with complete implementations**.

The implementation must be: fully functional, production-oriented, modular, performant, concurrent, async-first, type-safe, observable, testable, extensible, well documented, cleanly architected, and suitable for real-world deployment.

**Forbidden:** mock providers, fake API responses, placeholder implementations, `TODO`/`FIXME` implementations, stub/empty functions, `unimplemented!()`, `todo!()`, `panic!("not implemented")`, dummy integrations, hard-coded fake credentials, simulated provider behavior, pretend API clients, incomplete abstractions, "implement this later" comments, partial features presented as complete.

If an external service requires credentials, implement the **real integration and configuration mechanism** and clearly document the required environment variables. Where an external service cannot be exercised without credentials, the code must still compile and contain the actual production integration path rather than a fake implementation.

---

## 2. First Principle: Verify Before Implementing

Before writing substantial code:

1. Inspect the repository (docs, PRD, ADRs).
2. Determine current project structure and intended architecture.
3. Research current official documentation for **every** external API/provider integrated:
   - OpenAI (Chat Completions, Responses, streaming/SSE, tool calling, structured outputs, embeddings, fine-tuning jobs)
   - Anthropic (Messages API, streaming, tool use, prompt caching)
   - Google Gemini (generateContent, streamGenerateContent)
   - Mistral, Cohere (chat, embeddings)
   - OpenRouter (OpenAI-compatible, model list)
   - Ollama (local OpenAI-compatible)
   - MCP spec (Streamable HTTP / stdio, tools, resources, prompts)
   - A2A spec (agent cards, tasks)
   - Web standards relevant to crawling (robots.txt, sitemap, Content-Type, encodings)
4. Verify API contracts, auth mechanisms, request schemas, streaming protocols, error formats, rate limits.
5. Prefer official documentation and authoritative specifications. Do not invent APIs.
6. Record architectural and implementation decisions (ADRs; extend `ADRs/` with ADR-011+).

If documentation is unavailable or ambiguous, identify the uncertainty explicitly rather than inventing behavior.

---

## 3. Scope & Non-Goals

### In scope (this build)
- Monorepo workspace, all crates listed in §5, compiling with `cargo check --workspace`.
- Unified provider abstraction with **real** HTTP/SSE clients for: OpenAI, Anthropic, Google Gemini, OpenRouter, Ollama. (Mistral + Cohere adapters if API verification completes; otherwise documented as "requires credentials/verification" — never faked.)
- Streaming as first-class capability (SSE parsing, unified event stream).
- Parallel execution engine with concurrency control (fan-out/fan-in, race, fallback, deadlines, cancellation, retries, backoff, jitter, circuit breaker).
- Agent runtime: lifecycle states, instructions/context/state, tool loops, sub-agents, delegation, agent-to-agent (A2A), event emission, traces, HITL hooks.
- Tool system: typed schemas (JSON Schema), validation, permissions, timeouts, cancellation, tracing; built-in tools (http, fs, time, math, web); MCP client and server.
- Web subsystem: HTTP client, HTML fetch, robots-aware crawling, redirects, caching, content extraction (HTML→text), metadata, link discovery, concurrent crawl with limits, rate limiting; search provider trait with native (DuckDuckGo) backend — fully self-hosted, no external scraping API.
- Memory: 4-tier model with trait-based storage; in-process implementations; real external adapters where implemented (e.g., sqlite); embeddings via provider adapters.
- RAG: chunking strategies, ingestion, vector store trait (in-process + real adapters), hybrid/keyword retrieval, reranking interface, context assembly.
- Workflows: sequential/parallel/conditional/fan-out/fan-in, retries, timeouts, cancellation, checkpoints, state, result propagation.
- Observability: `tracing`-based structured logging, spans, chronological event history, exporters (in-memory, stdout JSON), request/trace/span IDs.
- Analytics: metrics (counts, latency, tokens, cost estimation, cache hit/miss, retries, errors, concurrency) aggregatable by provider/model/agent/tool/workflow/time window.
- Security: secret handling, redaction (API keys, headers, cookies, PII), URL/SSRF validation, tool permission boundaries, resource limits.
- Configuration: env vars + config files (TOML) + programmatic + runtime overrides; validation; fail-fast with useful errors.
- CLI (`ai-sdk`): `doctor`, `providers`, `models`, `config`, `run`, `inspect`, `trace`, `benchmark` — every command real.
- Tests: unit, integration (credential-gated), concurrency, property-based (proptest), benchmarks (criterion).
- Documentation: README (updated for Rust), docs/ guides, rustdoc across crates, examples that compile.

### Explicitly out of scope (documented, not faked)
- **Local LoRA fine-tuning / multi-LoRA serving / text-to-LoRA** (PRD §3.6.1–3.6.4): requires training infrastructure; only the real OpenAI fine-tuning jobs API adapter is implemented. Recorded as "Intentionally unsupported".
- **Computer use / browser automation / WebMCP** (PRD §3.7): deferred to roadmap. No fake browser layer.
- **Self-skill creation / meta-learning** (PRD §3.13): deferred; skill *framework* (registry, versioning) is in scope.
- **Voice full-duplex realtime** (PRD §3.5): `ai-voice` crate ships audio types, VAD, and STT/TTS traits with real REST adapters where verified; realtime WebSocket voice (e.g., OpenAI Realtime) is documented as requiring credentials + further verification.
- **IDE integration / LSP** (PRD §3.7.3): roadmap only.
- **Fine-tuning, browser automation, voice realtime** must appear in docs/limitations and in the engineering log — never as "complete".

---

## 4. Workspace & Repository Restructuring

The repository currently contains documentation only, plus a **dead `sdk` gitlink** (submodule entry with no `.gitmodules` URL). Restructure:

1. Remove the `sdk` gitlink entry from the index (`git rm --cached sdk`).
2. Create the Rust workspace at repo root: root `Cargo.toml` (workspace), `crates/`, `examples/`, `integration-tests/`, `benchmarks/`, `tests/`, `docs/`.
3. Keep `PRD-v1.md`, `VERIFICATION_REPORT.md`, `ADRs/`, `ENGINEERING-SPEC.md`; update `README.md` to describe the Rust SDK (or keep a pointer to docs while noting status).
4. Add ADR-011 (Rust language decision, superseding ADR-001) and ADR-012 (workspace restructuring).
5. `.gitignore`: `target/`, `.env`, secrets.
6. `LICENSE` (MIT, matching existing badge), `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`.

---

## 5. Monorepo Architecture

Rust workspace with strongly separated crates. Suggested layout (justified, may evolve with documented reasoning):

```text
F:\alisia\ai-sdk\
├── Cargo.toml                 # workspace
├── Cargo.lock
├── README.md / LICENSE / CHANGELOG.md / CONTRIBUTING.md / SECURITY.md
├── PRD-v1.md / VERIFICATION_REPORT.md / ENGINEERING-SPEC.md
├── ENGINEERING-LOG.md         # chronological execution report (§36)
├── ADRs/
├── docs/                      # architecture, providers, agents, tools, web,
│                              # memory, rag, workflows, observability,
│                              # analytics, security, performance, deployment
├── crates/
│   ├── ai-types/              # messages, content parts, roles, usage, modalities
│   ├── ai-core/               # core traits (Model, Provider, Tool), primitives,
│   │                          #   public re-exports hub
│   ├── ai-config/             # env + TOML + programmatic configuration, validation
│   ├── ai-errors/             # typed error hierarchy (§25)
│   ├── ai-models/             # model registry, metadata, capabilities, routing
│   ├── ai-providers/          # real adapters: OpenAI, Anthropic, Gemini, OpenRouter,
│   │                          #   Ollama (+ Mistral/Cohere pending verification)
│   ├── ai-runtime/            # parallel execution, concurrency limits, retries,
│   │                          #   backoff/jitter, deadlines, cancellation,
│   │                          #   fan-out/fan-in, race/fallback, circuit breaker
│   ├── ai-stream/             # unified streaming events, SSE parsing, transforms
│   ├── ai-tools/              # tool trait, JSON-Schema typed tools, built-ins,
│   │                          #   skill registry/versioning, MCP client+server glue
│   ├── ai-protocols/          # MCP client/server, A2A client/server, discovery
│   ├── ai-agents/             # agent runtime, lifecycle, sub-agents, patterns,
│   │                          #   swarms, self-healing, HITL, traces
│   ├── ai-web/                # HTTP client, crawler, extractor, parser, robots,
│   │                          #   cache, search providers (self-hosted)
│   ├── ai-memory/             # 4-tier memory, embeddings trait, storage trait,
│   │                          #   in-process + sqlite adapters, compaction
│   ├── ai-rag/                # chunking, ingestion, vector store trait, retrieval,
│   │                          #   hybrid search, reranking, context assembly
│   ├── ai-workflows/          # DAG engine: seq/parallel/conditional, checkpoints
│   ├── ai-observability/      # tracing, events, chronological history, exporters
│   ├── ai-analytics/          # metrics, aggregation, cost estimation
│   ├── ai-devtools/           # debugger/inspector, trace viewer, test generation
│   ├── ai-security/           # redaction, PII, SSRF/URL guards, validation
│   ├── ai-cache/              # cache trait, in-memory + (real adapter when verified)
│   ├── ai-storage/            # storage backends (kv, doc, vector traits)
│   ├── ai-edge/               # WASM/edge build targets, runtime detection
│   ├── ai-voice/              # audio types, VAD, STT/TTS traits + real adapters
│   ├── ai-cli/                # ai-sdk CLI: doctor, providers, models, config, run,
│   │                          #   inspect, trace, benchmark
│   └── ai-sdk/                # facade crate, unified public API
├── examples/                  # compiling examples (chat, agents, parallel, web,
│                              #   rag, workflows, cli)
├── integration-tests/         # credential-gated real-API tests
├── benchmarks/                # criterion: throughput, parallel, crawl, tools,
│                              #   events, serialization, cache
└── tests/
```

Key requirement: **clear separation of concerns and minimal unnecessary coupling**. Crates depend inward (`ai-types` → `ai-core` → feature crates); `ai-sdk` is the only broad facade.

---

## 6. Rust Engineering Requirements

- Modern stable Rust (MSRV pinned in `Cargo.toml`), idiomatic practices.
- `async`/`await` with Tokio; structured concurrency; `Result`-based errors; strong domain types; traits for extensibility; generics where they improve correctness.
- `Arc`; `RwLock`/`Mutex` only where justified; channels where appropriate.
- Cancellation, timeouts, backpressure, bounded concurrency, connection pooling, streaming, zero/low-copy designs where practical.
- **Avoid:** global mutable state, unnecessary locking, excessive cloning, blocking ops in async paths, unbounded queues, uncontrolled task spawning, hidden background tasks, resource leaks, excessive allocations, serializing inherently parallel workloads.
- Enforce: `cargo fmt`, `cargo check`, `cargo test`, `cargo clippy` (with `-D warnings` in CI).

---

## 7. Unified Model / Provider Abstraction

Implement the PRD §2.1 provider ecosystem behind a common interface:

```
Provider ──► Models ──► Capabilities ──► Requests ──► Responses / Streams
```

Capabilities (discoverable programmatically): text generation, chat, streaming, structured output, JSON/schema-constrained output, tool/function calling, vision/multimodal input, embeddings, model metadata, token usage, reasoning metadata, input/output modalities, provider-specific capabilities, provider-specific options without destroying portability.

**Do not force every provider into an artificially identical feature set.** Provider-specific options pass through typed extension points.

### Provider tiers (PRD §2.1)
- **Tier 1 (native adapters):** OpenAI, Anthropic, Google Gemini, Mistral, Cohere.
- **Tier 2 (community/adapters):** Groq, Together AI, Fireworks AI, Azure OpenAI, AWS Bedrock, GCP Vertex, Ollama, OpenRouter, custom OpenAI-compatible endpoints.

Minimal shipping set: **OpenAI, Anthropic, Google Gemini, OpenRouter, Ollama** — all real. Remaining adapters documented per status (verified/requires-credentials/pending).

---

## 8. Multi-Provider Parallel Execution

Parallel execution is first-class (§6 of template): multiple providers, models, tools, agent tasks, web operations, and retrieval operations in parallel, with **proper concurrency control** — not unlimited task spawning.

Support: configurable concurrency limits (global/per-provider/per-model/per-tool), request deadlines, cancellation propagation, retry policies, backoff, jitter, rate-limit handling, circuit breakers, failure isolation, partial results, aggregation, race/first-success, fallback, fan-out/fan-in.

Example conceptual API (design detail in §30):

```rust
let result = runtime
    .parallel()
    .model(openai_gpt4o)
    .model(anthropic_sonnet)
    .web(search_request)
    .tool(calculator)
    .execute()          // bounded concurrency, deadline, cancellation
    .await?;
```

---

## 9. Agent Development Kit

A complete agent runtime, not merely a chat client. PRD §3.3, §3.10, §3.11 requirements:

- Instructions, context, state, tool access, memory, planning, execution, delegation, multi-step workflows, structured outputs, tool loops, streaming, cancellation, timeouts, retry policies, human approval/intervention hooks, agent-to-agent communication, sub-agents, parallel sub-agents, event emission, execution traces.
- **Explicit lifecycle/state transitions** — no hidden magic.
- Patterns (PRD §3.3): hierarchical (supervisor/worker), pipeline, parallel (map-reduce), router (conditional), collaborative (group chat).
- Swarms (PRD §3.10): bounded parallel agent groups with coordination (centralized/decentralized), map-reduce and competitive modes.
- Self-healing (PRD §3.11): error classification, automatic recovery strategies (retry, compact, degrade model, decompose, escalate), circuit breaker.

---

## 10. Tool System

Real tool framework (template §8 + PRD §2.3):

- Strongly typed input schemas (JSON Schema), typed outputs where practical, metadata, descriptions, validation, permissions, timeouts, cancellation, resource limits, error propagation, observability, execution IDs, tool-call tracing.
- Built-in tools: http, fs, time, math, web/search, code (read/exec where permissioned).
- Clean `#[async_trait]`-style trait for user-defined tools.
- Skill registry (PRD §3.3): discovery by tags/capabilities, semver versioning, dependency resolution.

---

## 11. Built-In Web Capability

Serious built-in web/research subsystem (template §9), no fake abstraction:

- HTTP requests, HTML retrieval, robots-aware crawling, URL normalization, redirect handling, HTTP caching, content extraction, HTML→text conversion, metadata extraction, link discovery, concurrent crawling, rate limiting, timeouts, retries, content-size limits, MIME-type handling, encoding handling, search integration, page retrieval, structured extraction, web research workflows.
- Clear abstractions: `WebClient`, `SearchProvider`, `Fetcher`, `Crawler`, `Extractor`, `Parser`, `ContentNormalizer`, `RobotsPolicy`, `Cache`.

---

## 12. Advanced Web Research Layer (self-hosted)

Template §10 + PRD web research ambitions — implemented fully natively; no
external scraping service:

- Operations: `search`, `scrape`, `crawl`, `map`, `extract`, `research`.
- Support: single-page scraping, multi-page crawling, URL discovery, search, content extraction, structured extraction, concurrent page processing, crawl/depth limits, domain restrictions, include/exclude patterns, deduplication, caching, rate limiting, error recovery.
- Multiple backends behind a common trait:

```text
WebBackend
├── NativeHttpBackend   (real, implemented)
└── SearchBackend       (real, implemented)
```

Structured extraction is performed by the model via
`StructuredExtractor`/`LlmStructuredExtractor` — fully self-hosted.

---

## 13. Memory and Retrieval

PRD §3.4 four-tier model behind interfaces:

- Working (session), short-term (TTL), long-term (persistent), semantic (embeddings + vector store).
- Storage behind traits so users can integrate databases/vector stores without rewriting the agent runtime.
- In-process implementations always available; real external adapters where implemented (sqlite for long-term; vector store trait with an in-process implementation; external vector DB adapters documented by status).
- Compaction & summarization strategies (sliding window, summarization, hierarchical, importance-based).
- Embeddings via provider adapters (OpenAI etc.), never fake.

---

## 14. RAG Subsystem

PRD §3.8: chunking strategies (fixed, semantic boundaries, hierarchical), document ingestion, vector store trait, retrieval (dense/hybrid/keyword), reranking interface (cross-encoder/LLM rerank via providers), context assembly, optional GraphRAG/Self-RAG/C-RAG as documented roadmap items (implemented only if real).

---

## 15. Workflow Engine

Template §12: sequential, parallel, conditional branches, fan-out/fan-in, retry, timeout, cancellation, error handling, compensation where applicable, dependencies, result propagation, state, checkpoints. Long-running sessions with resume (PRD §3.8.2).

---

## 16. Protocols: MCP + A2A

PRD §3.1/§3.2, native support in `ai-protocols`:

- **MCP (2026-07-28, modern stateless revision — see ADR-013):**
  - Per-request `_meta` protocol version + client capabilities (REQUIRED);
    no `initialize` handshake.
  - `server/discover` (REQUIRED server method) + client version retry on
    `-32022 UnsupportedProtocolVersionError` (`data.supported`).
  - `resultType` on every result (`"complete"` | `"input_required"`).
  - **MRTR**: `InputRequiredResult { inputRequests, requestState }` with
    client `inputResponses` retry; elicitation (`elicitation/create`,
    form/url modes) capability-gated via `-32021`.
  - Subscriptions: `subscriptions/listen` with
    `io.modelcontextprotocol/subscriptionId` notification correlation.
  - Transports: line-delimited stdio (duplex) and Streamable HTTP
    (`MCP-Protocol-Version` header, JSON responses, SSE for listen streams,
    modern errors → HTTP 400).
  - Error codes `-32020`/`-32021`/`-32022`; JSON Schema 2020-12 default
    dialect; OTel trace-context `_meta` passthrough.
  - Deliberately not implemented: legacy initialize-handshake era
    (dual-era = roadmap), Roots (deprecated SEP-2577), HTTP+SSE (removed).
- **A2A client:** discover agents (agent cards), send tasks, receive
  artifacts/updates.
- **A2A server:** expose agents via A2A with skill/task handling.
- Unified discovery abstraction across both protocols.

Verified against the official `schema/2026-07-28/schema.ts`; no invented
protocol details.

---

## 17. Observability

Template §13: `tracing`-based structured logging (ERROR..TRACE, configurable by level and target). **No uncontrolled `println!`/`dbg!` noise.** Structured events with timestamp, request/trace/span IDs, provider, model, agent, tool, workflow, operation, duration, status, error classification, token usage, retry count, cache status, resource metrics.

---

## 18. Chronological Execution Reporting

Template §14: every significant AI execution produces a chronological event history (`00:00.000 …` style) as **structured telemetry**, not expensive string logging. Support in-memory collection, streaming events, exporters, trace correlation, chronological ordering, duration measurements, parent/child relationships, concurrent-operation correlation. Must stay performant under high concurrency.

---

## 19. Analytics

Template §15: request counts, success/failure, latency, throughput, provider/model latency, token usage, estimated cost (real pricing data where available), tool execution counts/latency, web requests, cache hit/miss, retry rates, error rates, concurrency, queue depth, agent/workflow durations. Aggregatable by provider/model/agent/tool/workflow/request/time window. Avoid high-cardinality metrics by default.

---

## 20. Debugging

Template §16: inspect requests, responses, tool calls, agent/workflow transitions, web ops, errors, retries, timing, token usage, provider selection, fallback behavior, parallel execution. Configurable redaction for API keys, authorization headers, cookies, credentials, personal data, provider secrets. Sensitive data must never be logged accidentally.

---

## 21. Performance Requirements

Template §17: high concurrency, low overhead, efficient memory, connection reuse, async I/O, streaming, bounded resources, efficient serialization, efficient event recording, parallel provider calls. No unsafe tricks without benchmarks. Criterion benchmarks for: request throughput, parallel provider execution, web crawling, tool dispatch, event recording, serialization, cache operations.

---

## 22. Resilience

Template §18: timeouts, retries, exponential backoff, jitter, rate-limit handling, cancellation, circuit breaking, provider fallback, partial failure handling, idempotency where appropriate, resource limits, backpressure. **Never retry blindly** — retry behavior configurable and error-class-aware.

---

## 23. Configuration

Template §19: environment variables, config files (TOML), programmatic configuration, provider-specific configuration, runtime overrides. Secrets never committed (`.env` gitignored; documented variable names). Clear validation; fail fast with useful errors when required config is missing.

---

## 24. Security & Compliance

Template §20 + PRD §4.2:

- Secret handling, SSRF risks, web crawling safety, URL validation, request limits, tool permissions, resource exhaustion, prompt/tool boundary issues, sensitive logging, user-provided URLs, file access, command execution (only via permissioned tools).
- PII detection/redaction, GDPR-style data handling interfaces (export, delete), audit log interfaces, data residency configuration (interfaces real; provider/backend-dependent parts documented).
- No dangerous capabilities without explicit permission boundaries.

---

## 25. Error System

Template §21, typed hierarchy:

```text
ConfigurationError · AuthenticationError · ProviderError · RateLimitError
TimeoutError · NetworkError · SerializationError · ValidationError
ToolError · WebError · StorageError · AgentError · WorkflowError
CancellationError
```

Errors preserve useful context, never leak secrets.

---

## 26. Streaming

Template §22 + PRD §2.2: unified structured events — `TextDelta`, `ToolCallStarted`, `ToolCallDelta`, `ToolCallCompleted`, `ReasoningDelta`, `UsageUpdate`, `Error`, `Completed`. Real SSE parsing per provider; users never consume raw provider formats. Async-stream (or similar) based, cancellable, backpressure-aware.

---

## 27. Testing

Template §23:

- **Unit:** types, provider abstractions, config, errors, tools, agent state, workflows, web parsing, caching, observability.
- **Integration:** provider adapters (credential-gated), web subsystem, agents, parallel execution, streaming, tool calling, storage, analytics.
- **Concurrency:** parallel requests, cancellation, timeouts, races, failure isolation, backpressure.
- **Property-based:** proptest where valuable.
- **Benchmarks:** criterion.
- External integration tests use real APIs only when credentials/config are supplied; otherwise compile + type validation with documented prerequisites.

---

## 28. Documentation

Template §24: README, architecture, getting-started, configuration, provider, agent, tool, web, memory, rag, workflow, observability, analytics, security, performance, deployment, troubleshooting guides, rustdoc API docs, examples. Examples must compile and use real APIs. Update README + docs for the Rust implementation; keep PRD/ADRs as design record.

---

## 29. CLI

Template §25 — `ai-sdk` binary, every command real:

```text
ai-sdk doctor        # environment + config health check
ai-sdk providers     # list configured providers/capabilities
ai-sdk models        # list models for a provider
ai-sdk config        # show/validate configuration
ai-sdk run           # run a script/agent file
ai-sdk inspect       # inspect a trace/execution
ai-sdk trace         # stream/export execution traces
ai-sdk benchmark     # run bundled benchmarks
```

---

## 30. API Design

Ergonomic, coherent, predictable (template §26):

```rust
let client = AiClient::builder()
    .provider(ProviderConfig::OpenAI { model: "gpt-4o".into() })
    .build()
    .await?;

let stream = client.generate(GenerateRequest::new("Explain Rust")).stream().await?;

let agent = Agent::builder()
    .model(...)
    .instructions(...)
    .tools([http_tool(), web_search_tool()])
    .memory(memory)
    .build()?;

let result = agent.run("Research and summarize").await?;

let result = runtime.parallel().model(...).model(...).web(...).tool(...).execute().await?;
```

These are architectural examples; final API designed for technical correctness.

---

## 31. Dependency Policy

Template §27: mature, actively maintained crates (tokio, reqwest, serde, tracing, thiserror, async-trait, url, scraper/selectors, sqlx or rusqlite, clap, criterion, proptest, etc.). Avoid unnecessary dependencies; justify each significant one. Do not reinvent mature foundational functionality without reason.

---

## 32. Code Quality

Template §28: idiomatic, modular, readable, consistent, documented, strongly typed, low-coupling, high-cohesion, testable. No enormous/monolithic modules. Public APIs intentionally designed. Traits where extension points are genuinely needed; no over-abstraction.

---

## 33. No Fake Completeness

Template §29, plus the repo's own **Hardcode Rules** from `VERIFICATION_REPORT.md`:

- No mocks, demos, fakes, stubs, placeholders, half-implementations, or TODOs.
- Every feature fully implementable or removed entirely.
- Warnings as errors, zero tolerance.
- Every file contains real, working, production-quality code.

Before declaring completion, search the entire repo for: `TODO`, `FIXME`, `todo!`, `unimplemented!`, `not implemented`, `stub`, `mock`, `fake`, `placeholder`, `dummy`, `coming soon`, `temporary`. Any remaining occurrence must be intentional and justified, or removed. No empty provider adapters to make the architecture look complete. No incomplete functionality hidden behind feature names.

---

## 34. Verification and Acceptance Criteria

Template §30, plus repo-wide audit:

```text
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --workspace --no-deps        # docs build
cargo build --examples                 # examples compile
```

Also verify: no broken imports, no dead critical code, no placeholders, no fake API responses, no accidental secret logging, no unbounded concurrency, no obvious resource leaks, no blocking ops in async paths; cancellation, timeouts, parallel execution, streaming, error propagation, provider isolation, web functionality, analytics, chronological tracing all work. Provider integrations that cannot be integration-tested without credentials pass compilation/type validation and document the external prerequisite.

---

## 35. Development Process

Template §31, incremental, repo always compiling:

1. Repository inspection & restructuring (remove dead submodule, ADR-011/012)
2. Workspace creation (root Cargo.toml, crates skeletons compile)
3. `ai-types` core type system
4. `ai-errors` + `ai-config`
5. `ai-core` traits + `ai-models`
6. HTTP/runtime infrastructure (`ai-web` http core, reqwest client)
7. Provider integrations (OpenAI first, then Anthropic, Gemini, OpenRouter, Ollama) + `ai-stream`
8. `ai-runtime` parallel execution + resilience
9. `ai-tools` + built-ins + skills registry
10. `ai-protocols` (MCP, then A2A)
11. `ai-agents` runtime + patterns + swarms + self-healing
12. `ai-web` crawler/extractor/search (self-hosted)
13. `ai-memory` + `ai-rag`
14. `ai-workflows`
15. `ai-observability` + chronological events
16. `ai-analytics`
17. `ai-security`
18. `ai-cache` + `ai-storage`
19. `ai-devtools`
20. `ai-edge` + `ai-voice`
21. `ai-cli`
22. `ai-sdk` facade
23. Examples + docs
24. Tests + benchmarks
25. Full validation (§34) + final audit + engineering log

At each stage: `cargo fmt && cargo check && cargo test` green.

---

## 36. Change and Execution Reporting

Maintain `ENGINEERING-LOG.md` chronologically. For each meaningful stage record: timestamp, component, change, reason, files/modules affected, dependencies added, tests executed, validation result, performance considerations, security considerations, known limitations. Distinguish: **Implemented / Verified / Requires external credentials / Intentionally unsupported / Known limitation**. Never disguise a limitation as successful implementation.

---

## 37. Final Deliverable

The actual complete Rust workspace in `F:\alisia\ai-sdk`: cloneable, configurable, buildable, testable, inspectable, extendable, deployable by a professional engineering team. Not an architecture document, file list, tutorial, pseudocode, partial implementation, generated scaffold, or collection of stubs. **Push to GitHub** (`ncdevshiv/ai-sdk`) so the repo contains the real SDK.

---

## 38. Engineering Judgment

Template §34. If a requested architectural decision would create poor performance, excessive coupling, security problems, unbounded resource consumption, provider lock-in, difficult testing, or unmaintainable code, replace it with a technically superior design and document the reasoning. Priority: 1 Correctness, 2 Real functionality, 3 Security, 4 Reliability, 5 Performance, 6 Maintainability, 7 Developer experience.

---

## 39. Absolute Requirements

**REAL IMPLEMENTATION ONLY. NO MOCKS. NO FAKE PROVIDERS. NO STUBS. NO TODO-BASED COMPLETION. NO PLACEHOLDERS. NO FAKE WEB CAPABILITY. NO FAKE ANALYTICS. NO DECORATIVE LOGGING. NO UNBOUNDED PARALLELISM. NO SECRET LEAKAGE. NO CLAIMS OF COMPLETENESS WITHOUT VERIFICATION.**

Build a genuinely functional, production-oriented, Rust-native AI SDK/ADK realizing the PRD v1.2 vision: multi-provider execution, parallel orchestration, agent capabilities, real web/research functionality, tools, memory, RAG, workflows, streaming, observability, analytics, debugging, security, testing, documentation, and a clean modular architecture.

---

## 40. PRD Compliance Matrix (target)

| PRD § | Feature | Crate(s) | Status target |
|---|---|---|---|
| 2.1 | Multi-provider (25+) | `ai-providers` | Implemented: OpenAI/Anthropic/Gemini/OpenRouter/Ollama; others documented |
| 2.2 | Streaming | `ai-stream` | Implemented + verified |
| 2.3 | Tool calling | `ai-tools` | Implemented + verified |
| 2.4 | Structured output | `ai-providers`/`ai-types` | Implemented + verified |
| 2.5 | Prompt registry/versioning | `ai-tools` (skills) / `ai-memory` | Registry+versioning implemented; A/B + DSPy optimization deferred |
| 3.1 | MCP | `ai-protocols` | Implemented (client+server) |
| 3.2 | A2A | `ai-protocols` | Implemented (client+server) |
| 3.3 | Subagents / patterns | `ai-agents` | Implemented + verified |
| 3.4 | Memory 4-tier | `ai-memory` | Implemented (in-process + sqlite), compaction verified |
| 3.5 | Voice | `ai-voice` | Audio/VAD/STT-TTS traits + adapters; realtime requires credentials |
| 3.6 | Fine-tuning | `ai-providers` (OpenAI jobs API) | Adapter implemented; local LoRA intentionally unsupported |
| 3.7 | Computer use / IDE | — | Deferred (roadmap) |
| 3.8 | RAG | `ai-rag` | Implemented (chunking/retrieval/hybrid); GraphRAG roadmap |
| 3.9 | Parallel tools / workflows | `ai-runtime`/`ai-workflows` | Implemented + verified |
| 3.10 | Swarms | `ai-agents` | Implemented (bounded) |
| 3.11 | Self-healing | `ai-agents`/`ai-runtime` | Implemented |
| 3.12 | Self-correction | `ai-agents`/`ai-evaluation` | Correction loops implemented; hallucination detection interface + strategies |
| 3.13 | Self-skill creation | — | Deferred (roadmap) |
| 4.1 | Observability | `ai-observability`/`ai-analytics` | Implemented + verified |
| 4.2 | Compliance | `ai-security` | PII/GDPR interfaces implemented; provider-dependent parts documented |
| 4.3 | Devtools | `ai-devtools` | Inspector/trace implemented; playground roadmap |
| 4.4 | Edge/WASM | `ai-edge` | Build targets + runtime detection implemented |
| 4.5 | Cost optimization | `ai-cache`/`ai-runtime` | Caching + routing + cost estimation implemented |
| 4.6 | Resilience | `ai-runtime` | Implemented + verified |
| 4.7 | Security | `ai-security` | Implemented |

---

*This spec supersedes the generalized template for the `ncdevshiv/ai-sdk` project. The template's spirit (real implementation, verification, no fake completeness) is preserved in full.*
