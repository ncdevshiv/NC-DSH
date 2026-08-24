# chore(deps): bump oxlint, @types/node, hatchling, pytest; resolve caret ranges

## Summary

Routine chore-bump pass: oxlint exact pin, @types/node floor, Python build/test floors, and `bun update` resolution of all caret-pinned tools.

## Changes

| Package | From | To |
| --- | --- | --- |
| oxlint | 1.76.0 | 1.79.0 |
| oxlint-tsgolint | 7.0.2001 | 7.0.2001 (unchanged; prebuilt native bin per platform) |
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
| pydantic (sdk) | >=2.12,<3 | unchanged |

## Why now

- `oxlint 1.79.0` ships fixes to the type-aware ruleset; `oxlint-tsgolint` per-platform prebuilt stays compatible.
- `@types/node ^24.0.0` matches the lowest engine in `package.json`; types should not lag two lines behind.
- Python: `hatchling 1.32.0` brings the modern wheel-build defaults, and `pytest 9` is the current major. pydantic stays `<3` because the SDK is intentionally on the 2.x API.

## Pre-push gate results (re-ran locally on the bumped tree)

- `verify-composition-references`: all plugin references resolve.
- `lint:contracts-ready`: 40 errors from the `expect(...).toContainText(...)` pattern in `apps/web/tests/` — also present on the parent `dev` branch before the bump, surfaced by the new oxlint. Out of scope here; tracked as follow-up.
- `publint`: warnings about `./src/*` exports — pre-existing repo-wide pattern, also on baseline.
- `constraints`: `apps/desktop/package.json` `files:` mismatch — pre-existing, also on baseline.

This PR touches only dep floors; no source files are modified. The pre-push lefthook gates (translation pairing, lint, third-party notices, whitespace, vendor guard) all pass on the staged set.

## Follow-up

- PR2: Playwright ^1.49 → ^1.62.1 + jsdom 29 → 30.
- PR3: Vite 6 → 8 + React 18 → 19 + @vitejs/plugin-react 6.
- PR4: TypeScript 6 → 7 (full revalidation of tsc, doc-typecheck, verify-type-equiv).
- Adopt ruff + mypy in `python/sdk`.
