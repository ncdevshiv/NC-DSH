# feat(deps): bump React 18→19, Vite 6→8, @vitejs/plugin-react 4→6 across the workspace

## Summary

Mechanically bumps every React + Vite floor across the 43 affected packages in one PR. Consolidates the web app on the same Rust-core toolchain (Rolldown, via Vite 8) that `tsdown` already uses, so the whole repo now bundles with one engine.

## Changes

### Per package (all 43)

| Dep | From | To |
| --- | --- | --- |
| react | ^18.2.0 | ^19.2.0 |
| @types/react | ~18.3.1 | ^19.2.0 |
| react-dom | ^18.2.0 (where pinned) | ^19.2.0 |
| @types/react-dom | ~18.3.0 (where pinned) | ^19.2.0 |

### Per app (apps/web only — apps/desktop uses no React directly)

| Dep | From | To |
| --- | --- | --- |
| vite | ^6.0.0 | ^8.2.0 |
| @vitejs/plugin-react | ^4.0.0 | ^6.1.0 |

## Validation on the bumped tree

- `typecheck:contracts-ready`: passes clean (exit 0, **0 TS errors**). React 19's stricter type contracts are fully compatible with the existing source — no `ref` forwarding fixes, no `useTransition` shape changes, no `defaultProps` deprecations to handle.
- Focused vitest on `packages/client/runtime` and `packages/client/ui-conversation` (the two largest JSX-heavy React packages): **53 files, 824/824 tests pass**.
- `apps/web` `vite build`: succeeds in 5.9s; output structure unchanged (vendor + langs + index split). The >500 kB chunk-size warning is pre-existing and not a regression of this PR.

## No source changes

This is a clean mechanical-only dep bump — 43 `package.json` files + `bun.lock` were modified, **no `src/`, `tests/`, or config files were touched**. A throwaway script (`scripts/do-bump-react-vite.mjs`) was used to apply the regex edits, then removed before commit.

## Why

- Vite 8 puts the app on Rolldown (the Rust core) — same engine `tsdown` already uses, so the whole repo bundles with one engine instead of two.
- React 19 rides along because `@vitejs/plugin-react 6.x` and updated `@types/react` want them paired, and `react-dom 19` brings the new concurrent features.
- Kills two major-version debts (Vite 6→8, React 18→19) while the project is pre-release — AGENTS.md's "foundation over blast radius" stance applies.

## Follow-up

- PR4: TypeScript 6 → 7 (full revalidation of tsc, doc-typecheck, verify-type-equiv).
- Adopt ruff + mypy in `python/sdk`.
