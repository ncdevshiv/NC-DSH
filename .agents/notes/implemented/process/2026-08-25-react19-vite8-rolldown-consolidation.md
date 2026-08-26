# Agent Note: React 19 and Vite 8 consolidate bundling on Rolldown

Status: implemented

English | [中文](2026-08-25-react19-vite8-rolldown-consolidation.zh.md)

## Problem

The workspace carried two major-version debts — Vite 6 and React 18 — and ran two bundling engines side by side: `apps/web` on Vite 6's JavaScript engine while everything `tsdown` bundles uses Rolldown (the Rust core). Every web-app build paid the second engine's maintenance cost and behavioral differences.

## Decision

One mechanical change raises every React and Vite floor across the 43 affected packages and consolidates the web app onto the same Rust-core bundling engine that `tsdown` already uses, so the whole repository bundles with one engine:

| Dependency | Scope | From | To |
| --- | --- | --- | --- |
| react | all 43 packages | ^18.2.0 | ^19.2.0 |
| @types/react | all 43 packages | ~18.3.1 | ^19.2.0 |
| react-dom | where pinned | ^18.2.0 | ^19.2.0 |
| @types/react-dom | where pinned | ~18.3.0 | ^19.2.0 |
| vite | apps/web only | ^6.0.0 | ^8.2.0 |
| @vitejs/plugin-react | apps/web only | ^4.0.0 | ^6.1.0 |

`apps/desktop` uses no React directly and stays untouched. React 19 rides along: `@vitejs/plugin-react` 6.x and the updated `@types/react` pair with it, react-dom 19 brings the concurrent features, and closing both majors in one change follows the pre-release "foundation over blast radius" stance. The diff stays mechanical: 43 `package.json` files plus `bun.lock`, with no `src/`, test, or config file touched. A throwaway script applied the regex edits and was removed before commit.

## Verification

- `typecheck:contracts-ready`: exit 0 with zero TypeScript errors — React 19's stricter type contracts need no `ref` forwarding fixes, no `useTransition` shape changes, and no `defaultProps` deprecation handling anywhere in the existing source.
- Focused vitest over the two largest JSX-heavy packages, `packages/client/runtime` and `packages/client/ui-conversation`: 53 files, 824/824 tests pass.
- The `apps/web` production build succeeds in 5.9s with the output structure unchanged (vendor + langs + index split); the >500 kB chunk-size warning pre-dates this change.

## Alternatives considered

**Bump Vite alone and keep React 18.** Rejected: `@vitejs/plugin-react` 6.x pairs with React 19, so consolidating only the bundler immediately re-introduces the version-pairing debt this single change removes.

**Keep Vite 6 for the app and consolidate nothing.** Rejected: that preserves exactly the two-engine split this decision deletes, leaving the web app as the one surface outside the Rust core with its own maintenance line.

**Stage through Vite 7 before 8.** Rejected: the app consumes no intermediate Vite 7 capability, so the extra hop adds a full review-and-revalidation cycle without closing either major-version debt any earlier.

## Consequences

The whole repository bundles on one Rust-core engine, and both major-version debts close while the project is still pre-release. Web builds inherit Rolldown's behavior and diagnostics from here on, and React 19's concurrent features are available to client packages without further floor work.
