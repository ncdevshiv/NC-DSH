# Self-Modification Capability Audit — Have / Partial / Lack

Objective under evaluation: DSH should know its own crashes and errors, know its users (persona, behavior, frustration) and their projects/problems, compile realtime self-reflection into concrete improvement proposals, live-edit its own composition without crashing, and ship its own replacements (spawn → test → cutover → retire). Evidence: three read-only source surveys plus direct verification this session (paths cited inline). Companion decision request: [`.agents/notes/proposed/feature/2026-08-25-self-modification-closed-loop.md`](.agents/notes/proposed/feature/2026-08-25-self-modification-closed-loop.md).

## Pillar scorecard

| # | Pillar | Verdict |
|---|---|---|
| P1 | Crash & error self-knowledge | **Partial** — strong seams, dormant by default, no host-crash capture |
| P2 | User persona / behavior / frustration | **Lack** (raw ingredients present, zero join) |
| P3 | Project & problem knowledge | **Partial** — excellent query engine, incomplete corpus |
| P4 | Realtime reflection → proposals | **Lack** (all inputs exist, no loop) |
| P5 | Live self-edit without crashing | **Partial** — real product for dynamic plugins only; not the core |
| P6 | Spawn → test → cutover → retire | **Partial** — version/cutover mechanics exist; testing + auto-rollback + survival missing |

## P1 — Crash & error self-knowledge

**Have.** Package-owned invariant registry (`packages/runtime-diagnostics/invariants`): every package ships an `./invariant` companion enforcing cold-load seeds + live-append checks, enforced at build time by `verify-package-invariants`. Session durability is crash-serious: JSONL temp-write→fsync→publish appends with torn-tail repair and synthetic interrupted-turn closers (`packages/session/session-persistence-jsonl/src/index.ts`), SQLite WAL + `synchronous=FULL` (`session-persistence-sqlite`), checkpoint policy fails closed before model streams and tool dispatch (`session-checkpoint-policy`). Telemetry seam projects first chunks + assembled records with severity from `tool/result.isError` / `turn/end` reasons and relays `agent/error` (`session-telemetry/src/coordinator.ts`), exported via OTLP in three modes (`session-telemetry-otel`), mounted in every shipped profile (`bundle/base/cordis.patch.yml:148-161`). Worker/subprocess deaths map to logical results (`workflow-worker-thread/src/host.ts`, `subprocess-local`); shell tools propagate exit/signal/timeout as model-readable text; boot installs fail-loud `unhandledRejection`.

**Lack (confirmed by grep).** No `process.on('uncaughtException')` handler in any src — an unexpected throw kills the host with only a stderr line. Telemetry delivery is best-effort with the durable outbox explicitly deferred (`session-telemetry/README.md:47`) — a hard crash silently loses the very records describing it. The invariant registry is **not mounted by any shipped bundle** (spine demo only). OTEL defaults DISABLED (env opt-in). No metrics/health endpoint, no `--verbose/--debug`, no error-aggregation surface (`sentry|bugsnag|crashReport` → zero).

**Close-the-gap moves:** mount `dsh-invariants` in `dsh-base`; add an uncaughtException/uncaughtRejection crash reporter writing a durable crash record (storage-domain) before exit; implement the deferred telemetry outbox; document/ship a debug flag.

## P2 — User persona / behavior / frustration

**Have (ingredients).** Per-message thumbs+note in a CAS-versioned storage-domain sidecar served over `/api/messageFeedback/*` (`packages/feedback/message-feedback`); `/feedback <text>` durable log-only events (`command-feedback`); every approval decision reconstructable per session (`approval/asked|decided|policy` pairs, `interaction/user-approval`); Q&A durable as tool call/result (`tool-ask-user`); a stable home-scoped anonymous UUID (`identity/anonymous-user-id`); namespaced hot-reloaded settings; agent-facing persona prose (`preset/persona`).

**Lack (zero-result greps).** `sentiment|frustrat*|satisfaction|NPS|CSAT|emotion|mood|userProfile|analytics` → nothing. No embeddings/vector retrieval for sessions anywhere. No user-profile entity; persona is static config prose that never learns.

**Close-the-gap moves:** a `user-model` storage-domain joining anonymous-id → workspaces → sessions → feedback/approval signals; extend `extractSessionEventText` (`session-query/src/extraction.ts`) with owner-defined semantics for `feedback/record`, `approval/*`, `session/title` so they become searchable; optional local embedding provider behind the existing search seam for true semantic recall.

## P3 — Project & problem knowledge

**Have.** `ctx.sessionQuery` is genuinely powerful today: live-preferred corpus over all sessions, cwd/time/parent filtering, cross-session FTS5 search with snippets, within-session event search with metadata pre-filters, full raw-log reads without revival, lineage trees + per-event provenance, title folds, zip export (`packages/session-query/*`; SQLite schema v8). Workspaces group sessions per canonical path with editable titles (`workspace/workspace`).

**Lack.** Extraction covers only core message/tool/todo/turn events — feedback, approvals, and titles are invisible to search. Workspaces carry no notes/status/goals/problem fields. Nothing joins "signals of trouble" (negative ratings, rejected approvals, repeated tool errors) to "the project where they happened".

**Close-the-gap moves:** widen extraction (as P2); add project-level annotation fields to the workspace domain; a problems roll-up view (per-workspace error/rating timelines) built on the existing read model.

## P4 — Realtime reflection → modification proposals

**Have (inputs).** Everything reflection needs is already captured: durable session logs, error severities, feedback, approvals, goals state machine (`goal/*`), logged plans (`plan-mode`), todos, workflow runs with log-only events, fresh-agent loops (`workflow/tool-ralph`).

**Lack (the loop itself).** No postmortem/self-reflection generator; no counterfactual "what would have prevented this" analysis; no pipeline that turns observations into concrete modification proposals. The repo's own culture already has the target format — Agent Notes and `dsh-find-simplifications` — but authoring them is purely human/model-manual today.

**Close-the-gap moves:** a `retrospective` capability (new seam or extension plugin) that consumes session-query + telemetry summaries after failures and emits structured proposals (candidate Agent Notes / preset patches / dynamic-package drafts) for review; schedule-based triggers already exist (`schedule`).

## P5 — Live self-edit without crashing

**Have (a real product).** Seven `cordis_*` tools define/activate/version/retire model-written plugins: immutable Packages `pkg-N` under versioned Plugins, host halves evaluated in fresh `node:vm` realms behind whitelist ctx façades, browser halves gated by frame-wide human approval, fibers under a dedicated group disposed on startup failure (nothing half-mounted), commit of `currentPackageId` only after complete activation, outcomes steered back to the owning agent, `@pluginId` mentions for in-place iteration (`packages/extensions/{tool-cordis,cordis-host-runner,cordis-client-runner}`).

**Verified limits vs. the vision.**
- Only **session-owned dynamic plugins** are touchable; built-in bundles/config cannot be edited by the running agent (`cordis-host-runner/src/index.ts:1232`, registry in-memory only).
- **Nothing survives restart** (registry Maps, `index.ts:1248` "lost on DSH restart"; page refresh starts clean).
- Rollback is **manual**: failed update keeps the old pointer without restarting it.
- **Not a security boundary**: host-realm helpers remain an escape (`sandbox.ts:1-12`); injected services are real; host-only packages activate with **no approval at all** (`index.ts:270-275`).
- Reach inside a run: register own marker-tagged tools, view others' schemas only; prompt sections only via injecting `systemPrompt`; no persona edit, no built-in removal.
- Hygiene: model-facing error text still names the retired tool (`cordis-host-runner/src/lifecycle.ts:39` references `cordis_runtime_inspect what:"temporary"`); the current design note is still marked `proposed` although implemented (`.agents/notes/proposed/architecture/2026-08-08-cordis-web-dynamic-packages.md`); the July note describes the retired trio.
- Mounting: web bundle mounts runners always; the toolset rides **only the `cordis` preset** — the `nio` preset mounts none of it.

**Close-the-gap moves:** persist dynamic packages (storage-domain backend + replay-on-boot with approval memory); a decided security stance for host-only activations; broaden reach deliberately (composition-file patching service with its own approval class) if core-editing is truly wanted; fix the stale error string; promote/reconcile the design notes.

## P6 — Spawn → test → cutover → retire

**Have.** Inside the runner: immutable versions, explicit `update` mode that retracts the old physical run before starting the new, commit-after-success, `stop`/`undefine` retirement, per-page single-activation orchestrator. Outside it, the same shape exists at distribution level: client-bundle HMR reload chains, versioned landlock prebuild release flow, Python runtime channel selection.

**Lack.** No automated **test-before-cutover** stage (spawn candidate → exercise → assert → only then promote); no automatic rollback trigger on failed health check; no restart-surviving cutover (ties to P5 persistence); no shadow/dual-run mode (cutover is retract-then-start); no health signal to gate promotion (ties to P1).

**Close-the-gap moves:** a canary verb on the runner (`run mode:'canary'` executing against a scratch scope with assertions, promoting on pass); auto-rollback wiring the existing kept-pointer mechanism to failure detection; persistence makes cutover survive restart.

## Cross-cutting findings this session (repo hygiene affecting trust in self-modification)

- `bun run typecheck`: **52 errors, all under `packages/client/**`** — React 19 ref-typing fallout (`RefObject<T | null>` vs legacy `Ref<T>`) in src (`JsonTree`, `ConversationRoot`, `WorkspaceBrowser`, `MenuView`, `PopupSelectView`, directory-picker clients) and six test specs referencing removed slot keys/props. The vite8-react19 validation used `typecheck:contracts-ready`, which does not compile the React face — validation-gate gap.
- Client test failures trace to **dual React**: `use-sync-external-store` (declared exact `1.2.0` by `ui-renderer`) keeps a nested `node_modules/react@18.3.1` because its peer range predates React 19; `@docsearch/react` carries another (website-side, harmless to tests). Symptom: `TypeError: Cannot read properties of null (reading 'useRef')` across sidebar/workspace/client-runtime specs. Fix direction: bump/widen the shim (≥1.4.x) or force-resolve a single React, then re-run.
- Suite-wide numbers taken mid-edit are unattributable (concurrent working-tree changes); re-measure serially after the tree settles.
- Doc-gate repairs completed this session: dead anchor fixed both languages, graphs regenerated, python pair resealed, misplaced PR-body notes converted to legal bilingual Agent Notes, budgets back under ceiling.

## Suggested build order (each phase independently valuable)

1. **See crashes**: mount invariants everywhere · crash-record reporter · telemetry outbox. *(P1)*
2. **Join user truth**: extraction widening · feedback indexing · user-model domain. *(P2+P3)*
3. **Reflect**: retrospective capability emitting reviewable proposals. *(P4)*
4. **Persist self-edits**: dynamic-package storage + boot replay + approval memory; security stance decided. *(P5)*
5. **Canary cutover**: canary mode + auto-rollback + health-gated promotion. *(P6)*

Phases 1–3 make the system honest about itself before phases 4–5 let it rewrite itself — deliberate order, since self-editing without self-knowledge just automates blind spots.
