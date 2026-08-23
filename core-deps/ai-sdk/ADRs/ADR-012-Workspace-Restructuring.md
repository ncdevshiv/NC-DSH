# ADR-012: Workspace Restructuring (docs-only repo → Rust workspace)

**Status:** Accepted
**Date:** 2026-08-09
**Deciders:** Project owner (Shivam Tiwari), ZCode engineering agent

## Context

The `ncdevshiv/ai-sdk` repository contained only documentation (README,
PRD-v1.md, VERIFICATION_REPORT.md, ADRs) plus a `sdk/` **gitlink entry** that
GitHub reports as a submodule with **no resolvable URL** (no `.gitmodules`
file). The actual SDK code was never pushed; anyone cloning the repo received
documentation and a dead submodule pointer.

## Decision

1. Remove the `sdk` gitlink from the index (`git rm --cached sdk`) — the entry
   has no `.gitmodules` mapping, so it is unrecoverable as a submodule and
   useless to consumers.
2. Restructure the repository as a **Cargo workspace** with the Rust SDK at
   the repo root: root `Cargo.toml`, `crates/` (25 crates), `examples/`,
   `integration-tests/`, `benchmarks/`, `tests/`, `docs/`.
3. Keep design documents (`PRD-v1.md`, `VERIFICATION_REPORT.md`, `ADRs/`,
   `ENGINEERING-SPEC.md`) as the design record; update `README.md` for the
   Rust implementation.
4. Document the prior TypeScript state in ADR-011; the verification report
   remains as a historical record of the TS-based design verification.

## Rationale

1. The submodule URL never existed, so preserving the gitlink serves no purpose.
2. A production SDK must be cloneable, buildable, and testable — the workspace
   layout delivers that.
3. Keeping docs alongside code preserves the design intent (PRD) that this
   build realizes.

## Consequences

### Positive

- Repo becomes a real, buildable SDK (per ENGINEERING-SPEC §37: final deliverable).
- Single source of truth for code + docs.

### Negative

- The old `sdk/` path no longer exists; any local checkout that had the
  submodule populated must re-locate code (the code was never on GitHub).
- README must be rewritten to avoid describing a TypeScript SDK that is not present.

## Related

- ADR-011 (language), `ENGINEERING-SPEC.md` §4 (restructuring).
