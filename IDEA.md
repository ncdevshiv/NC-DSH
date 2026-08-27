# DeepSeek Harness — Idea Board

> Generated 2026-08-27 for `deepseek-harness` (`@deepseek-ai/dsh-*` on vendored Cordis).
> Everything is a plugin — every idea below attaches to a documented seam, not a core patch.
> Read [`docs/architecture.md`](docs/architecture.md) and [`packages/README.md`](packages/README.md) before picking one up.

## How to use this file

- **Pick by appetite**: Quick Wins ship in a PR, Medium needs a design note, Big Bets want an Agent Note first.
- **Attach correctly**: each idea lists its `ctx` key / package seam so you wire through `ctx.effect()` / `ctx.on()` and avoid loop edits.
- **Log what the model sees**: any new model-visible input needs a `SessionEventMap` member (`model-visible ⟺ logged`).
- **One home per fact**: file the Agent Note beside the idea you implement; archive it when shipped.

---

## At a glance

| Bucket | Count | Ship horizon |
|---|---|---|
| Quick Wins | 8 | 1–3 days each |
| Medium | 10 | 1–2 weeks each |
| Big Bets | 7 | 1 month+ each |
| Moonshots | 3 | exploratory |

> Prioritization hint: `Impact = (user-visible gain × reuse across profiles) / blast radius`. Prefer seams that move web + headless + ACP together.

---

## Quick Wins (high leverage, low blast radius)

### 1. Vision capability auto-detection
**Tags:** `quick` `model-experience` `packages/llm`
**Problem:** User-added models seed with no `modalities` field; capability metadata is purely user-declared, so vision models read as text-only. Tracked in `dsh-vision-capability-root-cause`.
**Proposal:** Add heuristic inference from model id (`gpt-4o`, `claude-3.5-sonnet`, `gemini-*vision*`, `deepseek-vl`) at `resolve()` time, with explicit user override winning. Surface a `modalities: inferred | declared` badge in `apps/web` Models page.
**Touch points:** `packages/llm/*`, `packages/settings/*`, `apps/web` Models card.
**First slice:** Unit test the heuristic table; snapshot the badge.

### 2. Deletable providers + Nio i18n
**Tags:** `quick` `settings` `i18n`
**Problem:** Built-in DeepSeek provider is gated by `removable: false`; dual-path UI asymmetries remain. `apps/cli/config/agent-presets/nio` still has Chinese strings. Tracked in `dsh-provider-removal-nio-task`.
**Proposal:** Make every Models-page provider deletable end-to-end (settings seam + credential refs + `verify-cordis-config` manifest check). Translate `nio/` to English and add a `verify-nio-i18n` gate.
**Touch points:** `packages/credentials/*`, `packages/bundle/*`, `apps/cli`.
**First slice:** Flip `removable` gate + add regression test for delete → re-add round-trip.

### 3. Workspace delete cascade
**Tags:** `quick` `session` `data-integrity`
**Problem:** Deleting a workspace only removes registration; sessions become `Ungrouped` and the workspace resurrects via bootstrap `cwd` adoption.
**Proposal:** Add `ctx.workspaces.delete(id, { cascade: 'reassign' | 'delete-sessions' })` and a retention policy for orphan sessions. Bootstrap should not recreate a deleted id.
**Touch points:** `packages/workspace/*`, `packages/session/*`, `packages/storage/*`.
**First slice:** Add `delete` e2e that asserts no resurrection after restart.

### 4. Composer toolbar merge
**Tags:** `quick` `web-ui` `ux`
**Problem:** `+` / image / folder icons compete for space in `ui-conversation InputBar.tsx`.
**Proposal:** Merge into one smart `+` menu (attach image, attach folder, slash-command, skill). Keep a11y labels, keyboard nav, and drag-drop parity. Design already exists in `dsh-composer-toolbar-merge-task`.
**Touch points:** `packages/client/ui-conversation/*`.
**First slice:** Menu with 3 items + telemetry on open rate.

### 5. AppFrame height collapse
**Tags:** `quick` `web-ui` `layout`
**Problem:** `SpectrumSurface` Provider div breaks `height:100%`; shell collapses to ~318px.
**Proposal:** Make the provider flex-fill (`display:flex; flex:1; min-height:0`) and add a layout invariant test (pixel scan or `getBoundingClientRect` assertion).
**Touch points:** `packages/client/*`, `apps/web`.
**First slice:** One-line CSS fix + regression test.

### 6. Desktop titlebar WCO blend
**Tags:** `quick` `desktop` `polish`
**Problem:** Native Window Controls Overlay paints a mismatched rectangle over the header band; `capturePage` hides it, `PrintWindow` shows it.
**Proposal:** Blend overlay color with sidebar fill + header seam curve; verify with `PrintWindow(PW_RENDERFULLCONTENT)` pixel sampling. Tracked in `dsh-desktop-titlebar-zed-fix`.
**Touch points:** `apps/desktop`, `packages/client/*`.
**First slice:** Color token sync + screenshot-pixel test.

### 7. Bun 1.4 adoption hygiene
**Tags:** `quick` `tooling` `dx`
**Problem:** Repo runs on `bun 1.4.0` lockfile v2 but `engines.bun` still allows older; `Bun.YAML` fast-path needs a Node fallback audit.
**Proposal:** Bump `engines.bun >=1.4`, pin CI `setup-bun`, and add a `verify-bun-fallback` gate that forces `globalThis.Bun?.YAML` + `yaml@2` parity (never-throw `document.errors[]` handling).
**Touch points:** `package.json`, `scripts/*`, `packages/session/*` (yaml users).
**First slice:** Bump + gate; document in `docs/development.md`.

### 8. Session title provider extensibility
**Tags:** `quick` `session` `llm`
**Problem:** Only one `ctx.sessionTitle` provider can register; custom title strategies require forking.
**Proposal:** Promote to a waterfall (`session/title:resolve`) so heuristics, LLM, and user rules can compose. Keep current provider as default listener.
**Touch points:** `packages/session/*`, `packages/llm/*`.
**First slice:** Waterfall + test that two listeners compose.

---

## Medium (1–2 weeks, needs a design note)

### 9. OpenBot + Buzz Team integration (phased)
**Tags:** `medium` `agent-team` `experimental`
**Problem:** `Team` sidebar is thin (subagents + presets only); `core-deps/openbot+buzz` audits are done but not wired.
**Proposal:** Phase 1: per-bot computer + identity model behind `ctx.agentTeams` (private opt-in). Phase 2: task board UX. Phase 3: mailbox + delegation seam.
**Touch points:** `packages/experimental/*`, `packages/subagent/*`, `packages/client/*`.
**First slice:** Mount one buzz bot as a `subagent` provider; e2e delegation smoke.

### 10. Durable memory seam (OpenMEM sidecar)
**Tags:** `medium` `memory` `capability-seam`
**Problem:** No cross-session memory; session log is the only durable state.
**Proposal:** New capability seam `ctx.memory` — Service Definition + sidecar provider (OpenMEM) + `memory_search`/`memory_write` tools. Store is content-addressed, scoped per workspace, and model-visible via injected context (new session event).
**Touch points:** `packages/attachment/*`, `packages/session-query/*`, `packages/tools/*`.
**First slice:** File-backed provider + FTS search; snapshot the injected context.

### 11. Code graph retrieval (Dataworm)
**Tags:** `medium` `code-intelligence` `lsp`
**Problem:** File tools are `bash`-backed; no code-aware retrieval.
**Proposal:** `ctx.codeGraph` seam over Dataworm; `code_search` tool ranks by symbol graph + LSP. Results spill through `ctx.spill`.
**Touch points:** `packages/lsp/*`, `packages/fs/*`, `packages/spill/*`.
**First slice:** Read-only search over `packages/`; measure hit rate vs `grep`.

### 12. Compaction policy switcher
**Tags:** `medium` `compaction` `ux`
**Problem:** One basic compaction provider; no per-session policy.
**Proposal:** `ctx.compaction` with strategies (`basic`, `summarize+keep-tools`, `aggressive`) selectable per session/preset. Expose in settings card + command.
**Touch points:** `packages/compaction/*`, `packages/preset/*`, `packages/client/*`.
**First slice:** Two strategies + A/B snapshot diff.

### 13. Skill marketplace + hot-reload
**Tags:** `medium` `skill` `dx`
**Problem:** Skills are file-discovered; no browse/install UX and reload needs restart.
**Proposal:** Local registry UI (search, enable/disable, version), plus `ctx.skill` hot-reload via Cordis effect disposer. Verify with `verify-composition-references`.
**Touch points:** `packages/skill/*`, `packages/client/*`, `packages/host/*`.
**First slice:** Hot-reload for one local skill; add `skill/reload` event.

### 14. Terminal persistence & reattach
**Tags:** `medium` `terminal` `reliability`
**Problem:** PTY sessions die on reload; no reattach.
**Proposal:** Persist `ctx.terminals` sessions to `storage` backend and reattach on boot. Add `terminal/reattach` capability event and UI affordance.
**Touch points:** `packages/terminal/*`, `packages/storage/*`, `packages/client/ui-terminal/*`.
**First slice:** Persist + restore one shell session.

### 15. Approval policy presets v2
**Tags:** `medium` `guard` `security`
**Problem:** Approval presets are global; repo/path scoping is missing.
**Proposal:** Scoped presets (`global` < `workspace` < `repo` < `path`) with inheritance and `tools/*` waterfall enforcement. Add `verify-approval-scoping` gate.
**Touch points:** `packages/guard/*`, `packages/interaction/*`, `packages/fs/*`.
**First slice:** Path-scoped `fs/write` policy + test matrix.

### 16. Workflow/ralph resumable checkpoints
**Tags:** `medium` `workflow` `reliability`
**Problem:** `workflow`/`ralph` runs are not resumable after crash.
**Proposal:** Checkpoint each worker step to the session log; resume via `ctx.workflow.resume(id)`. Surface progress in `ctx.jobs` UI.
**Touch points:** `packages/workflow/*`, `packages/jobs/*`, `packages/session/*`.
**First slice:** One checkpoint + resume e2e.

### 17. Browser automation stability
**Tags:** `medium` `browser` `reliability`
**Problem:** `moli` CDP provider is flaky on navigation + screenshot timing.
**Proposal:** Retry with backoff, navigation fencing, and screenshot-diff wait. Add `browser/screenshot:diff` helper for UI tests.
**Touch points:** `packages/browser/*`, `packages/test-support/*`.
**First slice:** Retry wrapper + stabilize 3 existing web-GUI tests.

### 18. Unified attachment preview
**Tags:** `medium` `attachment` `web-ui`
**Problem:** Image/pdf/code previews are ad-hoc per surface.
**Proposal:** Single `attachment/preview` pipeline (validate → store → render) with ConversationNode renderers and spill-aware large-file handling.
**Touch points:** `packages/attachment/*`, `packages/spill/*`, `packages/client/*`.
**First slice:** Image + pdf preview parity across web and CLI.

---

## Big Bets (1 month+, needs an Agent Note + prototype)

### 19. Self-building orchestrator
**Tags:** `big-bet` `extensions` `self-mod`
**Problem:** Adding a plugin still needs a human to write `cordis.yml` and wire `ctx.*`.
**Proposal:** Live plugin authoring loop: model writes a plugin to `extensions/`, Cordis mounts it via `quiesce-and-rebind` hot-swap, blue/green fencing hands off sessions. Session log is the soul (promotion across source/artifact/live planes). See `self-building-orchestrator-plan`.
**Touch points:** `packages/extensions/*`, `packages/boot/*`, `packages/session/*`.
**First slice:** One generated tool plugin round-trips through mount/unmount with tests.

### 20. Agent Teams GA
**Tags:** `big-bet` `agent-team` `collab`
**Problem:** Experimental teams lack durable roster UX and cross-agent coordination.
**Proposal:** GA `ctx.agentTeams` with roster, task board, mailbox, and continuable subagents. Ship a Team preset and a `team/*` event map.
**Touch points:** `packages/subagent/*`, `packages/session/*`, `packages/client/*`.
**First slice:** Two-agent team completes a split task; snapshot the transcript.

### 21. Remote sandbox parity
**Tags:** `big-bet` `sandbox` `infra`
**Problem:** Local sandbox backends (`bwrap`/`Landlock`/`Seatbelt`) don't cover remote execution.
**Proposal:** Remote `ctx.fs` + `ctx.subprocess` + `ctx.shell` providers (E2B + owned Docker) with one execution world so Bash/PTY/LSP move together. No provider forks.
**Touch points:** `packages/sandbox/*`, `packages/fs/*`, `packages/subprocess/*`, `packages/shell/*`, `packages/e2b/*`.
**First slice:** Remote Bash + file read e2e against E2B.

### 22. Bun-native runtime
**Tags:** `big-bet` `perf` `bun`
**Problem:** Node shims block Bun built-ins (`Bun.open`, `Bun.Archive`, `Bun.YAML`, `Bun.Terminal`).
**Proposal:** Follow `dsh-bun-upstream-contribution-roadmap`: `Bun.open` starter → `Bun.Terminal` dogfood (delete `node-pty` patch) → `Bun.Archive` (kill `fflate`) → `Bun.YAML` positions/stringify → `Bun.Image` parity → `bun build --dts` proposal. Gate = `dsh` adopting Bun runtime.
**Touch points:** `native/*`, `python/*`, `packages/*` (zip/yaml/image users).
**First slice:** Ship `Bun.open` + `Bun.YAML` fast paths (done partially in PR #14); next is `Bun.Archive`.

### 23. Eval harness + model-upgrade snapshots
**Tags:** `big-bet` `testing` `llm`
**Problem:** Model upgrades regress transcripts silently.
**Proposal:** Keyless snapshot replay (`bun run test:snapshot`) + real-API eval suite with golden tasks; `gen-module-graph` freshness gate covers seam drift. Both SDKs track loop changes together.
**Touch points:** `packages/test-support/*`, `packages/sdk/*`, `python/*`, `examples/*`.
**First slice:** 10 golden tasks with stable expected outputs; CI gate on snapshot drift.

### 24. Session semantic search GA
**Tags:** `big-bet` `session-query` `search`
**Problem:** Session retrieval is bounded reads + FTS only.
**Proposal:** GA `session-query` with logical corpus, lineage, event relations, semantic filtering, and embedding-backed ranking. Ship SQLite FTS + vector index behind `ctx.sessionQuery`.
**Touch points:** `packages/session-query/*`, `packages/session/*`, `packages/storage/*`.
**First slice:** FTS GA + embedding ranker behind a flag; benchmark recall.

### 25. ACP headless fleet runner
**Tags:** `big-bet` `acp` `scale`
**Problem:** `dsh --profile headless` runs one task; no fleet.
**Proposal:** Shard tasks across an ACP fleet with `ctx.jobs` aggregation, retry with stepped backoff (`doubleEveryRetries`), and artifact spill collection.
**Touch points:** `packages/acp/*`, `packages/jobs/*`, `packages/host/*`.
**First slice:** 4-way shard of `test:snapshot:record` with merged report.

---

## Moonshots (exploratory, file an Agent Note to start)

### 26. Workspace memory federation
Federate `ctx.memory` + `ctx.identity` across workspaces with CRDT sync; durable roster and cross-workspace search. Needs privacy design first.

### 27. Real-time multimodal loop
Extend `agent/request` → `llm/stream` to carry audio/video chunks; new `SessionEventMap` members + `deriveMessages` projection. Prototype with one multimodal provider.

### 28. Signed remote plugin marketplace
Distribute `cordis.yml` patches as signed bundles; `verify-cordis-config` checks resolver manifest `dependencies` + signature. Enables `dsh --patch https://…` safely.

---

## Starter issues for new contributors

- [ ] `good first issue` — Fix AppFrame height collapse (#5) — one CSS line + test.
- [ ] `good first issue` — Composer toolbar merge (#4) — small UI, high polish.
- [ ] `good first issue` — Bun 1.4 engines bump (#7) — tooling only.
- [ ] `help wanted` — Vision heuristic table (#1) — model-experience, needs snapshot.
- [ ] `help wanted` — Workspace delete cascade (#3) — data-integrity, needs e2e.

---

## Parked / Revisit later

- **Rsbuild/Rspack migration** — declined 2026-08-24; stay on Vite/Vitest/tsdown until Rstest 1.x or measured HMR pain (`dsh-rsbuild-rspack-assessment`).
- **TypeScript 7 bump** — blocked on `eslint-plugin-sonarjs@4.2.0 → ts-api-utils@2.5.0` (`Type.Intrinsic` stub); reopen when upstream ships TS7 support (`dsh-typescript-7-blocker`).
- **pnpm → bun symlink probe** — design exists (`pnpm-to-bun-migration-audit`); needs go-ahead for junction/hardlink fallback.

---

## How to propose a new idea

1. Add a row to the right bucket with `Problem / Proposal / Touch points / First slice`.
2. Link the Agent Note if non-trivial (` .agents/notes/active/...`).
3. Run `bun run doc-sync` if you touched `docs/` references, and `bun run verify-composition-references` if you added a plugin row.
4. Open a PR with `kind/feature` + relevant `area/*` labels.

---

*Keep this file lean — one paragraph per idea, one home per fact. Archive shipped ideas to `.agents/notes/archived/` and remove them here.*
