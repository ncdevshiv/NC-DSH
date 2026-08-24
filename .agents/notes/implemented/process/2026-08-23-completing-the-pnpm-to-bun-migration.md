# Agent Note: Completing the pnpm-to-bun migration across CI, release lanes, and product surfaces

Status: implemented

English | [中文](2026-08-23-completing-the-pnpm-to-bun-migration.zh.md)

## Problem

The repository switched its package manager to bun (`packageManager: bun@1.4.0`, `bun.lock`, root-level `overrides`, `patchedDependencies`, and `trustedDependencies`) but only the local toolchain surface moved with it. Every automation lane that installs or spawns through a package manager still drove pnpm: five ci.yml jobs, ten other GitHub workflows, the GitLab Python-runtime wheel lane, the Wine Windows gate script, and most of `scripts/`. Because `pnpm-lock.yaml` was deleted, none of those lanes could install at all — `actions/setup-node cache: pnpm` hard-fails without it, `pnpm/action-setup` cannot parse a `packageManager` field naming bun, and `pnpm install --frozen-lockfile` has nothing to freeze against. Two code paths also re-spawned child scripts through `process.env.npm_execpath`, which under bun points at the standalone `bun.exe`: launching that file under node dies with a SyntaxError on the PE bytes, breaking `bun run build` and every gate aggregate. The result was a repository where local development worked, `bun run build` failed on a verified path, and every CI lane and both release pipelines were dead on arrival.

## Decision

Bun is the only package manager any lane drives. GitHub workflows install with `oven-sh/setup-bun@v2` plus an explicit `actions/cache` step keyed on `~/.bun/install/cache` and `hashFiles('bun.lock')` (the arch-qualified key survives where setup-node's built-in cache needed platform separation), then run `bun install --frozen-lockfile`. The GitLab runtime-wheel jobs download the bun release pinned by the root manifest's `packageManager` field and run the same frozen install. Scripts that must re-invoke the invoking package manager detect a standalone-binary entrypoint (an `.exe` suffix, or a `bun/`-prefixed `npm_config_user_agent`) and spawn it directly; node-backed managers keep the JavaScript-entrypoint-under-node form. Exec-style invocations under bun target the workspace-installed launcher in `node_modules/.bin` directly, because `bun exec` resolves PATH entries only.

Three designs changed shape rather than translating flag-for-flag:

- `scripts/build-exe-for-python-sdk.ts` stages the runtime closure as a standalone mini-workspace — the closure manifest plus every transitive workspace package copied under `deps/`, installed with `--production --ignore-scripts --linker=hoisted` — replacing `pnpm deploy --legacy --prod`; the pre-existing link-materialization pass turns the workspace symlinks into real files.
- `dsh plugin` forwards to bun instead of pnpm (`why` maps to `bun pm why`), profiles initialize a `bunfig.toml` pinning `linker = "hoisted"` instead of `pnpm-workspace.yaml`, and the git-dependency failure guidance points at `trustedDependencies` in the profile `package.json` instead of `allowBuilds`.
- `scripts/wine-windows-gates.sh` installs its scratch snapshot with `--os=win32 --cpu=x64 --linker=hoisted`, which reproduces what pnpm's `supportedArchitectures` plus hoisted-nodeLinker overrides did; bun's own flags carry both, and the upstream rename-race retry loop has no counterpart to port.

`verify-vendored-links` parses `bun.lock` as JSONC (line comments and trailing commas) and asserts workspace specs instead of `link:` entries; `gen-third-party-notices` reads member globs and patches from the root manifest and resolves license metadata from the installed tree including bun's `.bun/node_modules` store. `rescope-vendor`'s postconditions pin the root-manifest `overrides` entries, and its vendor README prose describes that resolution mechanism.

## Alternatives considered

Keeping pnpm solely for `pnpm deploy` and the Landlock pack split was rejected because no lane can bootstrap pnpm anymore — corepack refuses a `packageManager` field naming bun, and the lockfile it needs is gone. Routing `bun x` for Vitest and tsx invocations was rejected because bunx executes JavaScript bins under the Bun runtime, while every lane here runs them under Node; the faithful equivalents are existing package scripts (`bun run test:e2e <files>`) and the repo-standard `node --import tsx/esm` launcher. Leaving the Wine lane on pnpm was rejected for the same bootstrap reason, and bun's `--os`/`--cpu` install overrides plus explicit hoisted linking reproduce the required win32-x64 binary materialization in two flags.

## Consequences

Every install path now depends on one toolchain, one lockfile, and one trust model: `trustedDependencies` gates lifecycle scripts everywhere, so the manylinux node-pty rebuild mounts only the node-gyp header cache and no longer mounts the retired setup directory. Bun's default isolated linker is a live constraint — anywhere a flat layout matters (the Wine snapshot, the staged runtime closure) passes `--linker=hoisted` explicitly, and `verify-vendored-links` plus the notices generator read the `.bun/node_modules` store when the hoisted tree does not name a package. Release-lane behavior that only a real pack-and-install can prove — `bun pm pack` filename and exec-bit handling for the Landlock entry tarballs, and `bunx @yao-pkg/pkg` driving SEA packaging under the Bun runtime — is exercised by the release workflows and the packed-install rehearsals rather than by unit coverage. The `scripts/build.ts` and gate-runner fixes were validated by simulating their exact spawn shape under bun; full end-to-end `bun run build` additionally requires a repository with commits, since the client build record binds the HEAD hash.
