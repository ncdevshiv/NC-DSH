# ADR-011: Rust Implementation Language

**Status:** Accepted
**Date:** 2026-08-09
**Deciders:** Project owner (Shivam Tiwari), ZCode engineering agent

## Context

ADR-001 originally selected TypeScript/Node.js as the primary implementation
language for the AI SDK. The `VERIFICATION_REPORT.md` describes a TypeScript
codebase with 371 passing tests. During the 2026-08-09 build session, the
project owner decided to implement the SDK in **Rust** instead, following the
master engineering template ("Senior Rust AI SDK / ADK Engineering
Specification") adapted to our aim (`ENGINEERING-SPEC.md`).

## Decision Drivers

1. Performance and resource efficiency requirements (PRD §Performance: p95 < 500ms, 10,000 RPM/instance, < 100MB footprint) favor a compiled, zero-cost-abstraction language.
2. Parallel execution, streaming, and bounded concurrency are first-class requirements; Tokio provides mature async infrastructure.
3. Single static binary distribution (`ai-sdk` CLI) simplifies deployment.
4. Memory safety without a GC suits long-running agent runtimes and edge/WASM targets (PRD §4.4).
5. The generalized engineering template used for this build is Rust-specific.

## Considered Options

1. **TypeScript/Node.js** (ADR-001, superseded): fastest ecosystem adoption; was the original choice; deviates from the Rust template.
2. **Rust** (chosen): template-aligned, performance, safety, single binary.
3. **Hybrid (Rust core + TS bindings)**: most ambitious; ADR-001 deferred it; adds ABI/binding complexity and slows delivery.

## Recommendation

**Rust (stable, edition 2024, MSRV 1.85)** as the implementation language for
the entire SDK workspace. Rationale:

1. Directly satisfies the performance/concurrency/streaming requirements.
2. Aligns with the adopted engineering specification.
3. Enables future WASM/edge targets without rewrites.

## Consequences

### Positive

- Performance-critical paths (parallel fan-out, SSE parsing, crawling, event recording) run with minimal overhead.
- One toolchain (`cargo fmt/check/test/clippy`) for the whole monorepo.
- Real CLI binary distribution.

### Negative

- Supersedes ADR-001; TypeScript community/ecosystem reach is not exploited.
- No JS bindings in this build (roadmap item).
- Developer onboarding for JS-first contributors is steeper.

## Related

- ADR-012 (workspace restructuring), ADR-001 (superseded), `ENGINEERING-SPEC.md` §0.
