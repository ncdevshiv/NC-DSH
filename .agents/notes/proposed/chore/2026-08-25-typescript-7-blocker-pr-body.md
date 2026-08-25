# docs: TypeScript 7 is gated on third-party tooling (blocked, do not retry alone)

## Summary

Investigated the TypeScript 6 → 7 bump in response to the upgradation plan. **Found three blockers — one hard, one workable, one already validated.** Shipped the investigation as a proposed Agent Note rather than a code change, because the hard blocker cannot be resolved in this repository.

## Status

- **No code change in this PR.** Branch `feat/typescript-7-2026-08-25` is at the same source SHA as `dev`; only the new Agent Note is added.
- The branch is pushed so the work-in-progress isn't lost; the note captures the full finding for whoever picks this up next.

## What the investigation found

### Hard blocker: `eslint-plugin-sonarjs` cannot load under TS7

`ts-api-utils@2.5.0` (the latest) is loaded by `eslint-plugin-sonarjs@4.2.0` and unconditionally accesses `Type.Intrinsic` on the `typescript` package import. TS7's npm package ships only `lib/getExePath.{d.ts,js}`, `lib/tsc.js`, and `lib/version.{cjs,d.cts}` — there is no `Type` enum, no `ScriptTarget`, no Compiler API at all. The crash is reproducible from the oxlint binary:

```
Failed to parse oxlint configuration file.
  x Failed to load JS plugin: eslint-plugin-sonarjs
  |   TypeError: Cannot read properties of undefined (reading 'Intrinsic')
  |     at Object.<anonymous> (node_modules/ts-api-utils/lib/index.cjs:787:57)
```

This blocks **every oxlint invocation in the workspace**, including the lefthook pre-commit `lint:contracts-ready` gate. Eight `sonarjs/*` rules are enabled in `.oxlintrc.json` and have no 1:1 oxlint-native equivalents.

The fix requires one of:
- A new `ts-api-utils` release that handles the TS7 stub, **or**
- A new `eslint-plugin-sonarjs` release that pins an older `ts-api-utils` for TS7 users, **or**
- Replacing sonarjs with native oxlint rules (regresses lint coverage).

### Workable: private `typescript-v6` alias for the custom type gates

TS7's npm package has no Compiler API. The repo's custom type gates that drive the API directly (verify-type-equiv, doc-typecheck, verify-cordis-config, verify-client-packages, publint-all, the central ts-project helper used by 15+ scripts, the vitest shared transformer, the sqlite test, the runtime Typert generator) can be made to work by importing the legacy `typescript@6.0.3` package under a private `typescript-v6` alias:

```sh
bun add --dev 'typescript-v6@npm:typescript@6.0.3'
# in each affected file:  import ts from 'typescript-v6'
```

`tsc -b` keeps using the new TS7 driver. The legacy driver is held only for these specific scripts. Validated locally: every gate above passes after the alias swap. This is the same pattern `oxlint-tsgolint` already uses (per-platform prebuilt native binary, not coupled to the npm `typescript` package).

### Stricter type contracts surface six errors in two files

TypeScript 7's stricter checks surface 6 errors in 2 files when the codebase is rebuilt. All are real and fixable:

- **4× `TS1361` in `packages/extensions/cordis-host-runner/src/index.ts`**: the file defines brand functions `CordisDynamicPluginId`, `CordisDynamicPackageId`, `CordisDynamicPluginRunId`, `ApprovalRequestId` *and* imports their matching type aliases with `import type { ... } from './types.ts'`. TS6 silently merged the two; TS7 refuses. Fix: move the brand functions to `types.ts` and import them as values, alongside the type-only import.
- **`TS2578` + `TS2769` in `packages/client/ui-conversation/tests/views-type-chain.client.spec.tsx`**: TS7 correlates a slot registration's component prop signature with its `inject` callback return, surfacing a stricter error message for a test that was already intentionally negative. Fix: switch the `@ts-expect-error` to a stacked `@ts-ignore` with a comment explaining the new correlation.

## Decision

Do not land the TS7 bump today. The first blocker is unsolvable on our side: `ts-api-utils` and `eslint-plugin-sonarjs` are third-party packages outside this repository, and there is no fork or local override that would not cost more than it saves. The AGENTS.md "foundation over blast radius" stance still applies: prefer the correct foundation, but a foundation that breaks the entire lint pipeline is not the correct foundation.

Reopen the note when any of:
- `ts-api-utils` ships a release that handles the TS7 stub, or
- `eslint-plugin-sonarjs` ships a TS7-compatible release, or
- Oxlint ships native rules covering the eight sonarjs rules currently enabled.

## Validation of the workable parts (private alias + source fixes)

Done locally on a working tree at the same commit, then reverted:

- `bun run typecheck:contracts-ready` → exit 0
- `bun run verify-type-equiv` → 392 type-equiv blocks pass
- `bun run doc-typecheck:contracts-ready` → 82 blocks compile, 731 type-equiv blocks skipped (checked elsewhere), 894 paired derivatives
- `bun run verify-cordis-config` → 131 config files pass
- `bunx vitest run packages/session/session-persistence-sqlite/tests/sql-resource-boundary.spec.ts` → 2/2 pass via the `typescript-v6` driver

The blocker part is reproducible by running the workspace's `oxlint` binary on any source file with the current `.oxlintrc.json`.
