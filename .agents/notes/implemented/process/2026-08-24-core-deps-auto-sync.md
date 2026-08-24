# Agent Note: core-deps auto-sync from upstream branches

Status: implemented

English | [中文](2026-08-24-core-deps-auto-sync.zh.md)

## Problem

NIO's dependency substrate under `core-deps/` is plain directory copies of `github.com/ncdevshiv/dataworm`, `openmem`, and `ai-sdk`. None carried `.git` metadata, nothing recorded which commit a copy corresponded to, and the one updater in the tree (dataworm's `dw up`) reinstalls the *global* tool, not these copies. Pushes to the upstream branches therefore had no effect locally: the sidecars kept executing stale code with no signal that newer features or fixes existed.

## Decision

`core-deps/sync.mjs` owns the update lifecycle; `core-deps/run-synced.mjs` is the boot wrapper both MCP sidecar rows in `agent.cordis.yml` launch through. Every NIO run, before the interpreter starts, applies any deferred swap, runs a TTL-cached `git ls-remote` drift check against the pinned branch (default 6 hours, `DW_COREDEPS_TTL_HOURS`), and — under the default policy `DW_COREDEPS_MODE=auto` — shallow-clones and applies that branch so the run executes upstream HEAD. `notify` only logs; `off` skips all network; `DW_COREDEPS_OFFLINE=1` forces silent offline pass-through.

The safety properties are proven by `sync.mjs selftest` against a real local git remote: runtime-local state (`.venv/`, openmem's root `config.json`, `data/`, `.dataworm/`, `.nc-code/`, compiled `.pyd`/`.so`, SQLite/graph databases, caches) is never overwritten or deleted; deletions are limited to the previous update's manifest recorded in `core-deps/sync-state.json`; replaced sources land in `.backup/<project>/<timestamp>/` first; a failed import smoke gate (`dataworm.cli`, `mcp_server`) rolls back. Files locked by running processes defer via a `.pending/` marker that the next boot applies before any sidecar exists. Pins, manifests, and the check cache persist in `sync-state.json`; `bun run coredeps:{check,update,status}` drive the same engine by hand.

Updates replace silent staleness with explicit advisories: changed `rust/` means the compiled extension needs a rebuild (or `--no-rust` fallback), changed `pyproject.toml` means dependencies may need refreshing, and ai-sdk source changes require rebuilding the separate `nio-gateway` build.

## Alternatives considered

- **Proper git clones inside `core-deps/`.** Rejected: keeping bare directories plus a state file preserves the deployment shape — absolute sidecar paths in cordis.yml, machine-local data living inside the project directories, and the fact that this repository does not track those trees.
- **Self-update inside each Python sidecar** (the `dw up` pattern). Rejected: it duplicates the engine per language, and an in-process updater can never affect the already-running sidecar; applying before spawn (the wrapper) is the only point where an update can take effect for the current run.
- **Check-only with manual apply.** Rejected as the default by the owner — every run must execute latest; it remains available as `DW_COREDEPS_MODE=notify`.
- **cron/CI scheduled sync.** Rejected: drift would land at times unrelated to runs, and a per-run check is still needed for freshness guarantees.

## Consequences

Every fresh start now executes whatever the pinned branch holds, moving upstream quality gates from "someone remembered to copy" to "before the session begins"; a broken upstream HEAD fails its own smoke gate and rolls back instead of taking the agent down. The cost side: the first update per project pays one full clone (dataworm's vendored browser-engine tree being the heaviest), the native fast path and gateway binary still rebuild manually after Rust changes, and reproducibility now depends on `sync-state.json` recording what actually ran — deleting it re-baselines silently. Trusting these repositories means trusting their owner: the supply chain is exactly `github.com/ncdevshiv/*`.
