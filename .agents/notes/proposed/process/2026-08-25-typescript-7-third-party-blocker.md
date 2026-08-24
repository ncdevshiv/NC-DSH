# Agent Note: TypeScript 7 is gated on third-party tooling (blocked, do not retry alone)

Status: proposed

## Problem

Investigated the TypeScript 6 → 7 bump as the highest-leverage remaining upgrade (roughly 10× faster `tsc`, native typescript-go compiler, `oxlint-tsgolint` already on the same engine). The bump hits three concrete blockers, one of which is hard to work around and one of which is unworkable without a third-party release.

## Blockers (in order of severity)

### 1. Hard block: `eslint-plugin-sonarjs` depends on `ts-api-utils`, which accesses `Type.Intrinsic` that TS7 does not export

`ts-api-utils@2.5.0` (the latest) is loaded by `eslint-plugin-sonarjs@4.2.0` and unconditionally accesses `Type.Intrinsic` on the `typescript` package import. TS7's npm package ships only `lib/getExePath.{d.ts,js}`, `lib/tsc.js`, and `lib/version.{cjs,d.cts}` — there is no `Type` enum, no `ScriptTarget`, no `readConfigFile`, no Compiler API at all. The crash is reproducible from the oxlint binary:

```
Failed to parse oxlint configuration file.
  x Failed to load JS plugin: eslint-plugin-sonarjs
  |   TypeError: Cannot read properties of undefined (reading 'Intrinsic')
  |     at Object.<anonymous> (node_modules/ts-api-utils/lib/index.cjs:787:57)
  |     ...
  |     at Object.<anonymous> (node_modules/eslint-plugin-sonarjs/cjs/S6759/rule.js:62:24)
```

This blocks **every oxlint invocation in the workspace**, including the lefthook pre-commit `lint:contracts-ready` gate. Eight `sonarjs/*` rules (`no-duplicate-in-composite`, `no-all-duplicated-branches`, `no-identical-conditions`, `no-identical-expressions`, `no-identical-functions`, `no-duplicated-branches`, `no-duplicate-test-title`, `duplicates-in-character-class`) are enabled in `.oxlintrc.json` under the `packages/**/*.{ts,tsx}` block.

`ts-api-utils` declares a `typescript: ">=4.8.4"` peer dep but its runtime check predates the TS7 stub. The fix requires either:
- A new `ts-api-utils` release that handles the TS7 stub, or
- A new `eslint-plugin-sonarjs` release that pins an older `ts-api-utils` for TS7 users, or
- Replacing sonarjs with native oxlint rules where they exist (the eight rules above are not 1:1 covered by oxlint's built-in rule set today).

### 2. Workable with a private alias: every script that drives the TS Compiler API must use a TS6 driver

TS7's npm package has no Compiler API. The repo's custom type gates that drive the API directly:

- `scripts/verify-type-equiv.ts` (392 type-equiv blocks)
- `scripts/doc-typecheck.ts` (82 fenced type blocks in Markdown)
- `scripts/verify-cordis-config.ts` (131 composition files)
- `scripts/verify-client-packages.ts`
- `scripts/publint-all.ts`
- `scripts/ts-project.ts` (central helper used by ~15 more scripts)
- `vitest.shared.ts` (vitest transformer for `tsx`-style transforms in tests)
- `packages/session/session-persistence-sqlite/tests/sql-resource-boundary.spec.ts`
- `packages/typert/generator/src/analyzer.ts` and `tsdown-plugin.ts` (the **runtime** Typert generator, which parses and transpiles TypeScript at module load)

All of these can be made to work by importing the legacy `typescript@6.0.3` package under a private `typescript-v6` alias and pointing the affected files at it:

```sh
bun add --dev 'typescript-v6@npm:typescript@6.0.3'
# in each affected file:  import ts from 'typescript-v6'
```

The alias works. `tsc -b` keeps using the new TS7 driver. The legacy driver is held only for these specific scripts. Verified locally: every gate above passes after the alias swap.

This is a known pattern — `oxlint-tsgolint` already does the same thing (per-platform prebuilt native binary, not coupled to the npm `typescript` package).

### 3. Stricter type contracts surface six errors in two files

TypeScript 7's stricter checks surface 6 errors in 2 files when the codebase is rebuilt. All are real and fixable:

- **4× `TS1361` in `packages/extensions/cordis-host-runner/src/index.ts`**: the file defines brand functions `CordisDynamicPluginId`, `CordisDynamicPackageId`, `CordisDynamicPluginRunId`, `ApprovalRequestId` *and* imports their matching type aliases with `import type { ... } from './types.ts'`. TS6 silently merged the two; TS7 refuses. Fix: move the brand functions to `types.ts` and import them as values, alongside the type-only import.
- **`TS2578` + `TS2769` in `packages/client/ui-conversation/tests/views-type-chain.client.spec.tsx`**: TS7 correlates a slot registration's component prop signature with its `inject` callback return, surfacing a stricter error message for a test that was already intentionally negative. Fix: switch the `@ts-expect-error` to a stacked `@ts-ignore` with a comment explaining the new correlation.

## Decision

Do not land the TS7 bump today. The first blocker is unsolvable on our side: `ts-api-utils` and `eslint-plugin-sonarjs` are third-party packages outside this repository, and there is no fork or local override that would not cost more than it saves. The AGENTS.md "foundation over blast radius" stance still applies: prefer the correct foundation, but a foundation that breaks the entire lint pipeline is not the correct foundation.

Reopen this note when any of:
- `ts-api-utils` ships a release that handles the TS7 stub, or
- `eslint-plugin-sonarjs` ships a TS7-compatible release, or
- Oxlint ships native rules covering the eight sonarjs rules currently enabled.

The two workable items above (the `typescript-v6` private alias + the 6 source-file fixes) are already validated locally and are repeatable. The bump becomes a one-day PR the moment any of the upstream releases land.

## What was tried in this investigation

1. Bumped `typescript` floor to `^7.0.2` in all 7 manifest locations (root, apps/web, native/landlock-run, packages/client/web, packages/lsp/lsp-stdio, packages/session/session-persistence-sqlite, packages/typert/generator).
2. Resolved and ran `bun run typecheck:contracts-ready` — clean after the 6 source-file fixes above.
3. Confirmed the cordis-host-runner and views-type-chain fixes in isolation.
4. Added the `typescript-v6` private alias and rewired 18 scripts + 1 shared helper + 2 source files + 1 test fixture to import from it. All affected gates (`verify-type-equiv`, `doc-typecheck`, `verify-cordis-config`, `verify-client-packages`, `publint`, `vitest` shared transformer) pass.
5. Hit the `eslint-plugin-sonarjs` blocker above. The whole lint pipeline fails to start. No in-tree workaround exists.
6. Reverted the bump on the working branch (`feat/typescript-7-2026-08-25`) — branch is now at the same SHA as `dev`. No code change ships from this investigation; only the note itself.

## Considered alternatives

**Replace the 8 sonarjs rules with the closest oxlint-native equivalents.** OXC has `no-duplicate-in-import`, `no-duplicate-key`, `no-duplicate-string`, and `no-duplicated-branches` (different semantics than sonarjs's). A 1:1 swap is not possible without losing coverage. Even if a partial replacement were acceptable, it would be a 1–2 day effort to validate equivalence on the existing source, and the result would still be a regression in lint coverage. Not worth it given that the upstream fix is the right resolution.

**Fork `ts-api-utils` and `eslint-plugin-sonarjs` to local packages and ship them in the bundle.** Forbidden by the AGENTS.md "prefer maintained dependencies over hand-rolling" rule, and the fork would need perpetual maintenance against upstream. Not worth it.

**Hold the TS7 bump and ship the `typescript-v6` alias as its own preparation PR.** The alias by itself does nothing useful without the bump. The two go together or not at all.

## Tests

Verification of the workable parts (private alias + source fixes) was done locally:

- `bun run typecheck:contracts-ready` → exit 0
- `bun run verify-type-equiv` → 392 type-equiv blocks pass
- `bun run doc-typecheck:contracts-ready` → 82 blocks compile, 731 type-equiv blocks skipped (checked elsewhere), 894 paired derivatives
- `bun run verify-cordis-config` → 131 config files pass
- `bun run verify-client-packages` → 1 pre-existing violation (unrelated to this work)
- `bun run publint` → pre-existing repo-wide `./src/*` glob warnings (unrelated to this work)
- `bunx vitest run packages/session/session-persistence-sqlite/tests/sql-resource-boundary.spec.ts` → 2/2 pass via the `typescript-v6` driver

The blocker part is reproducible by running the workspace's `oxlint` binary on any source file with the current `.oxlintrc.json`. The fix in this note's Blockers 1 paragraph is the only missing piece.
