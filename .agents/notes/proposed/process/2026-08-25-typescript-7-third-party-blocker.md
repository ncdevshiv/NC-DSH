# Agent Note: TypeScript 7 is gated on third-party tooling (blocked, do not retry alone)

Status: proposed

English | [中文](2026-08-25-typescript-7-third-party-blocker.zh.md)

## Problem

The TypeScript 6 → 7 bump is the highest-leverage remaining upgrade — roughly 10× faster `tsc`, the native typescript-go compiler, and `oxlint-tsgolint` already on the same engine — but it hits three concrete blockers: one unworkable without a third-party release, one workable with a private alias, and one ordinary source work.

## Blockers

### Hard block: eslint-plugin-sonarjs cannot load under TS7

`ts-api-utils@2.5.0` (the latest release), loaded by `eslint-plugin-sonarjs@4.2.0`, unconditionally accesses `Type.Intrinsic` on the `typescript` package import. TS7's npm package ships only `lib/getExePath.{d.ts,js}`, `lib/tsc.js`, and `lib/version.{cjs,d.cts}` — there is no `Type` enum, no `ScriptTarget`, no `readConfigFile`, no Compiler API at all. The crash reproduces from the oxlint binary:

```
Failed to parse oxlint configuration file.
  x Failed to load JS plugin: eslint-plugin-sonarjs
  |   TypeError: Cannot read properties of undefined (reading 'Intrinsic')
  |     at Object.<anonymous> (node_modules/ts-api-utils/lib/index.cjs:787:57)
  |     ...
  |     at Object.<anonymous> (node_modules/eslint-plugin-sonarjs/cjs/S6759/rule.js:62:24)
```

This blocks every oxlint invocation in the workspace, including the lefthook pre-commit `lint:contracts-ready` gate. Eight `sonarjs/*` rules (`no-duplicate-in-composite`, `no-all-duplicated-branches`, `no-identical-conditions`, `no-identical-expressions`, `no-identical-functions`, `no-duplicated-branches`, `no-duplicate-test-title`, `duplicates-in-character-class`) are enabled in `.oxlintrc.json` under the `packages/**/*.{ts,tsx}` block. `ts-api-utils` declares a `typescript: ">=4.8.4"` peer dep, but its runtime check predates the TS7 stub. Only an upstream release resolves this: a `ts-api-utils` version that handles the TS7 stub, an `eslint-plugin-sonarjs` version that pins an older `ts-api-utils` for TS7 users, or oxlint-native rules covering the sonarjs ones where they are enabled.

### Workable with a private alias: every Compiler-API driver needs a TS6 driver

TS7's npm package has no Compiler API, and the repo's custom type gates drive it directly:

- `scripts/verify-type-equiv.ts` (392 type-equiv blocks)
- `scripts/doc-typecheck.ts` (82 fenced type blocks in Markdown)
- `scripts/verify-cordis-config.ts` (131 composition files)
- `scripts/verify-client-packages.ts`
- `scripts/publint-all.ts`
- `scripts/ts-project.ts` (central helper used by ~15 more scripts)
- `vitest.shared.ts` (vitest transformer for `tsx`-style transforms in tests)
- `packages/session/session-persistence-sqlite/tests/sql-resource-boundary.spec.ts`
- `packages/typert/generator/src/analyzer.ts` and `tsdown-plugin.ts` (the runtime Typert generator, which parses and transpiles TypeScript at module load)

Importing legacy `typescript@6.0.3` under a private `typescript-v6` alias and pointing these files at it works: `tsc -b` keeps the new TS7 driver, and the legacy driver is held only for these specific scripts. This is a known pattern — `oxlint-tsgolint` already decouples the same way with its per-platform prebuilt native binary.

### Stricter type contracts surface six real errors in two files

Rebuilding on TS7 surfaces six errors, all real and fixable: 4× `TS1361` in `packages/extensions/cordis-host-runner/src/index.ts`, where the brand functions `CordisDynamicPluginId`, `CordisDynamicPackageId`, `CordisDynamicPluginRunId`, and `ApprovalRequestId` collide with same-named type aliases imported through `import type { ... } from './types.ts'` (TS6 merged the two silently; TS7 refuses; the fix moves the brand functions into `types.ts` and imports them as values alongside the type-only import); and `TS2578` + `TS2769` in `packages/client/ui-conversation/tests/views-type-chain.client.spec.tsx`, where TS7 correlates a slot registration's component prop signature with its `inject` callback return, surfacing a stricter error on an intentionally negative test (the fix switches `@ts-expect-error` to stacked `@ts-ignore` with a comment explaining the new correlation).

## Proposal

Hold the TS7 bump. The first blocker is unsolvable inside this repository: `ts-api-utils` and `eslint-plugin-sonarjs` are third-party packages outside this repo, and no fork or local override would cost less than it saves. The pre-release "foundation over blast radius" stance still applies, but a foundation that breaks the entire lint pipeline is not the correct foundation. The `typescript` floor stays at 6.x across all seven manifest locations: root, `apps/web`, `native/landlock-run`, `packages/client/web`, `packages/lsp/lsp-stdio`, `packages/session/session-persistence-sqlite`, and `packages/typert/generator`.

Reopen this note when any of these lands upstream:

- `ts-api-utils` ships a release that handles the TS7 stub;
- `eslint-plugin-sonarjs` ships a TS7-compatible release;
- oxlint ships native rules covering the eight enabled sonarjs rules.

The moment any of them lands, the bump becomes roughly a one-day PR, because both workable parts are already validated and repeatable. For the Compiler-API drivers, the validated recipe imports legacy TypeScript under a private alias:

```sh
bun add --dev 'typescript-v6@npm:typescript@6.0.3'
# in each affected file:  import ts from 'typescript-v6'
```

rewires 18 scripts, 1 shared helper, 2 source files, and 1 test fixture; the six stricter-contract fixes above repeat the same way.

## Investigation evidence

The investigation bumped the `typescript` floor to `^7.0.2` in all seven manifest locations, confirmed the cordis-host-runner and views-type-chain fixes in isolation, added the `typescript-v6` alias with the rewiring described above, and verified locally:

- `bun run typecheck:contracts-ready` → exit 0 after the six source-file fixes;
- `bun run verify-type-equiv` → 392 type-equiv blocks pass;
- `bun run doc-typecheck:contracts-ready` → 82 blocks compile, 731 type-equiv blocks skipped (checked elsewhere), 894 paired derivatives;
- `bun run verify-cordis-config` → 131 config files pass;
- `bun run verify-client-packages` → 1 violation, pre-existing and unrelated to this work;
- `bun run publint` → pre-existing repo-wide `./src/*` glob warnings;
- `bunx vitest run packages/session/session-persistence-sqlite/tests/sql-resource-boundary.spec.ts` → 2/2 pass through the `typescript-v6` driver.

Then the investigation hit the sonarjs blocker: the whole lint pipeline fails to start, and no in-tree workaround exists. The bump was reverted on the working branch (`feat/typescript-7-2026-08-25`, now at the same SHA as `dev`), so no code change ships from this investigation — only the note itself. The blocker reproduces by running the workspace oxlint binary on any source file under the current `.oxlintrc.json`.

## Alternatives considered

**Replace the eight sonarjs rules with the closest oxlint-native equivalents.** OXC offers `no-duplicate-in-import`, `no-duplicate-key`, `no-duplicate-string`, and a `no-duplicated-branches` with different semantics than sonarjs's, so a 1:1 swap is impossible without losing coverage; even a partial replacement costs one to two days of equivalence validation on the existing source and still regresses lint coverage while the upstream fix remains the right resolution.

**Fork `ts-api-utils` and `eslint-plugin-sonarjs` into local vendored packages.** Forbidden by the prefer-maintained-dependencies rule, and the fork would need perpetual maintenance against upstream.

**Ship the `typescript-v6` alias alone as a preparation PR.** The alias does nothing useful without the bump; the two go together or not at all.

## Acceptance criteria

The hold state stays observable: the `typescript` floor reads 6.x in all seven manifest locations named under Proposal; the oxlint pipeline — including the lefthook `lint:contracts-ready` pre-commit gate — runs green with the eight sonarjs rules enabled; and this note reopens once an upstream release named under Proposal lands, with the alias recipe and the six source fixes revalidated against the then-current script inventory before the bump lands.

## Risks

The hold defers the roughly 10× `tsc` speedup from every developer and CI typecheck run. Sonarjs rule coverage stays frozen: improvements touching those eight rules cannot ship while the note holds. The alias recipe rots: as scripts join or leave the Compiler-API driver set, the validated rewiring inventory (18 scripts, 1 helper, 2 sources, 1 fixture) drifts out of date, so reopening requires re-auditing the affected-file list rather than replaying it blindly.
