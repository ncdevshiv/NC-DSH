# Agent Note: Toolchain floor bump wave across JS and Python tooling

Status: implemented

English | [中文](2026-08-25-toolchain-floor-bump-wave.zh.md)

## Problem

Dev-tool floors in both ecosystems had drifted: the exact-pinned `oxlint` lagged current type-aware rule fixes, `@types/node` sat on the 22.x line while `package.json` declares engines `^22.19 || >=24`, the Python build/test floors (`hatchling`, `pytest`) trailed current majors, and every caret-pinned dev tool resolved to a lockfile entry of whatever age it had. Contributors and CI therefore ran tool versions whose lint and type behavior differed from current upstream.

## Decision

A single chore pass raised each floor to its current floor-line version and resolved all caret ranges through `bun update`:

| Package | From | To |
| --- | --- | --- |
| oxlint | 1.76.0 | 1.79.0 |
| oxlint-tsgolint | 7.0.2001 | 7.0.2001 (unchanged; prebuilt native binary per platform) |
| @types/node (root) | ^22.20.0 | ^24.0.0 |
| @types/node (apps/web) | ^22.0.0 | ^24.0.0 |
| mermaid | 11.16.0 | 11.17.0 |
| knip | ^6.16.1 | resolved 6.32.2 |
| tsdown | ^0.22.2 | resolved 0.22.14 |
| publint | ^0.3.21 | resolved 0.3.24 |
| jscpd | ^5.0.12 | resolved 5.0.16 |
| fast-check | ^4.8.0 | resolved 4.9.0 |
| hatchling (sdk + sdk-runtime) | 1.30.1 | 1.32.0 |
| pytest (sdk) | >=8.0 | >=9.0 |

The drivers: oxlint 1.79.0 ships type-aware ruleset fixes and its oxlint-tsgolint per-platform prebuilt stays compatible; `@types/node ^24.0.0` matches the lowest declared engine instead of trailing two lines behind; hatchling 1.32.0 carries the current wheel-build defaults; pytest 9 is the current major. `pydantic` deliberately stays at `>=2.12,<3` because the SDK is intentionally on the 2.x API. The change touches dependency floors only — no source file changes.

The same wave produced sibling decisions recorded separately: jsdom 30 under Vitest's wildcard peer ([testing](../testing/2026-08-25-jsdom-30-under-vitest-wildcard-peer.md)), the React 19 / Vite 8 Rolldown consolidation ([process](2026-08-25-react19-vite8-rolldown-consolidation.md)), ruff and mypy adoption ([process](2026-08-25-python-ruff-mypy-adoption.md)), and the TypeScript 7 bump held by a third-party blocker ([proposed](../../proposed/process/2026-08-25-typescript-7-third-party-blocker.md)).

## Verification

On the bumped tree, `verify-composition-references` passes (all plugin references resolve), and the pre-push lefthook set — translation pairing, lint, third-party notices, whitespace, vendor guard — passes on the staged set. Three signals that the stricter tooling surfaces pre-date the bump on the parent `dev` branch and are owned outside this note: 40 `expect(...).toContainText(...)` errors in `apps/web/tests/` under the new oxlint ruleset, `publint` warnings about `./src/*` exports (a repo-wide pattern), and an `apps/desktop/package.json` `files:` mismatch under the constraints gate.

## Alternatives considered

**Hold each floor until a feature demands it.** Rejected: stale types against the engine range and outdated lint rules are silent costs paid by every run rather than by the bump itself, and the pre-release stance prefers correcting the foundation over carrying drift.

**Split each package into its own bump change.** Rejected: floor bumps are independent of source code, so per-package reviews add overhead without isolating risk, and batching keeps lockfile resolutions consistent in one pass.

## Consequences

Contributors and CI run current lint, type, build, and test tooling with lockfile resolutions pinned at the raised floors. The stricter oxlint ruleset keeps three pre-existing baseline defects visible in gate output; those defects remain owned outside this note. Future floor bumps start from this table rather than from pre-wave drift.
