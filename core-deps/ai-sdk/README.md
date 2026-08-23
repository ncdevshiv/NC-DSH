# AI SDK — Rust

> The world's most comprehensive AI SDK — multi-provider, multi-agent, production-ready.
> **Now implemented in Rust** per [ADR-011](./ADRs/ADR-011-Rust-Implementation-Language.md).

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.85+-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/Status-Verified-green.svg)]()
[![CI](https://github.com/ncdevshiv/ai-sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/ncdevshiv/ai-sdk/actions/workflows/ci.yml)

## Overview

This workspace contains the AI SDK, a comprehensive **Rust** SDK/ADK for
building AI-powered applications and agents. It provides a unified interface
across multiple LLM providers, parallel execution, agent orchestration,
memory, RAG, web research, workflows, streaming, observability, analytics,
and security — with a real, production-oriented implementation (no mocks,
no stubs, no placeholders).

- **Product Requirements Document:** [`PRD-v1.md`](./PRD-v1.md)
- **Engineering Specification (adapted):** [`ENGINEERING-SPEC.md`](./ENGINEERING-SPEC.md)
- **Architecture Decision Records:** [`ADRs/`](./ADRs/)
- **Design verification report (historical):** [`VERIFICATION_REPORT.md`](./VERIFICATION_REPORT.md)

## Crate Layout

| Crate | Description |
|---|---|
| [`ai-types`](crates/ai-types) | Core domain types: messages, content parts, roles, usage, modalities |
| [`ai-core`](crates/ai-core) | Core traits: Model, Provider, Tool, Client, runtime abstractions |
| [`ai-config`](crates/ai-config) | Unified configuration: env vars, TOML files, programmatic |
| [`ai-errors`](crates/ai-errors) | Typed error hierarchy |
| [`ai-models`](crates/ai-models) | Model registry, metadata, capabilities, routing |
| [`ai-providers`](crates/ai-providers) | Real adapters: OpenAI, Anthropic, Google Gemini, OpenRouter, Ollama |
| [`ai-runtime`](crates/ai-runtime) | Parallel execution: concurrency limits, retries, circuit breaker |
| [`ai-stream`](crates/ai-stream) | Unified streaming events, SSE parsing |
| [`ai-tools`](crates/ai-tools) | Tool framework, built-in tools, skills registry |
| [`ai-protocols`](crates/ai-protocols) | MCP client/server, A2A client/server |
| [`ai-agents`](crates/ai-agents) | Agent runtime, sub-agents, patterns, swarms, self-healing |
| [`ai-web`](crates/ai-web) | Web subsystem: crawler, extractor, search (self-hosted) |
| [`ai-memory`](crates/ai-memory) | 4-tier memory with pluggable storage |
| [`ai-rag`](crates/ai-rag) | RAG: chunking, ingestion, retrieval, hybrid search |
| [`ai-workflows`](crates/ai-workflows) | Workflow engine: sequential/parallel/conditional, checkpoints |
| [`ai-observability`](crates/ai-observability) | Structured logging, spans, chronological event history |
| [`ai-analytics`](crates/ai-analytics) | Metrics, cost estimation, aggregation |
| [`ai-devtools`](crates/ai-devtools) | Inspector, trace viewer, debugging |
| [`ai-security`](crates/ai-security) | Redaction, PII, SSRF guards, permissions |
| [`ai-cache`](crates/ai-cache) | Caching: TTL, semantic cache interface |
| [`ai-storage`](crates/ai-storage) | Storage backends: KV, document, vector (sqlite adapter) |
| [`ai-edge`](crates/ai-edge) | Edge/WASM build targets, runtime detection |
| [`ai-voice`](crates/ai-voice) | Voice: audio types, VAD, STT/TTS traits + adapters |
| [`ai-cli`](crates/ai-cli) | CLI: `doctor`, `providers`, `models`, `config`, `run`, `inspect`, `trace`, `benchmark` |
| [`ai-sdk`](crates/ai-sdk) | Facade crate: unified public API |

## Getting Started

> **Status:** Implemented across 25 Rust crates — ~270 unit tests plus a
> 16-test credential-gated live-gateway suite. CI runs rustfmt, clippy,
> and workspace tests; the live-gateway job runs only when its gateway
> secrets are configured (gated on `env.AI_SDK_GATEWAY_API_KEY`).

```bash
# Build the workspace
cargo check --workspace

# Run tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets --all-features
```

## Quick Links

- [Engineering Specification](./ENGINEERING-SPEC.md)
- [PRD v1.2](./PRD-v1.md)
- [Architecture Decision Records](./ADRs/)
- [Changelog](./CHANGELOG.md)
- [Contributing](./CONTRIBUTING.md)
- [Security](./SECURITY.md)

## License

MIT — see [LICENSE](./LICENSE).
