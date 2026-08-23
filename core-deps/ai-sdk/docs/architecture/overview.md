# Architecture Overview

The AI SDK is a Rust workspace of focused crates:

- **Core**: `ai-types` (domain types), `ai-errors` (typed error hierarchy),
  `ai-config` (env/TOML/programmatic configuration), `ai-core` (Model/
  Provider traits, `AiClient`), `ai-models` (model registry + pricing).
- **Execution**: `ai-runtime` (parallel executor, retries, circuit breaker),
  `ai-stream` (SSE parsing, unified events), `ai-providers` (real
  OpenAI-compatible adapters), `ai-tools` (tool framework + built-ins).
- **Agents**: `ai-agents` (agent runtime, sub-agents, swarms, self-healing),
  `ai-memory` (four-tier memory), `ai-rag` (chunking/retrieval/hybrid
  search), `ai-workflows` (DAG engine with checkpoints).
- **Protocols**: `ai-protocols` (MCP 2026-07-28 stateless client/server,
  A2A client/server).
- **Web**: `ai-web` (fetch, crawl, robots, extraction, search, research
  backends; fully self-hosted).
- **Operations**: `ai-observability` (events/chronological reports),
  `ai-analytics` (metrics), `ai-devtools` (inspector/traces), `ai-security`
  (redaction, PII, SSRF), `ai-cache`, `ai-storage` (KV/doc/vector, sqlite).
- **Edge/voice**: `ai-edge` (runtime detection), `ai-voice` (VAD, STT/TTS).
- **Interfaces**: `ai-cli` (real commands), `ai-sdk` (facade).

See `ENGINEERING-SPEC.md` for the full specification and `ADRs/` for
decisions (notably ADR-011 Rust, ADR-012 restructuring, ADR-013 MCP
2026-07-28 rewrite).
