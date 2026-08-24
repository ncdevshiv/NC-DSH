# NIO Blueprint

NIO is the self-building orchestration agent of this deployment: a harness
whose control plane can task itself to extend, repair, and replace its own
runtime under deterministic safety gates. This file is NIO's working
masterplan. It records the target architecture, what exists today, and what is
left to build.

The composition this directory mounts gives NIO today's surface: goals,
subagent delegation, model-authored workflows over worker threads, background
jobs, skills, plan mode, shell, filesystem, and web search. Everything below
that line is roadmap.

## Governing rules

1. The model proposes; deterministic runtime components validate, stage,
   swap, and roll back. No LLM edits running code in place.
2. Every self-change is an ordinary task: contract, acceptance criteria,
   executed evidence, independent evaluation. Self-changes get special gates,
   never special privileges.
3. Three planes, promoted through gates:
   - **Source plane** — versioned code, configs, prompts, skills.
   - **Artifact plane** — validated build outputs (plugin bundles).
   - **Live plane** — currently executing registry bindings.
   A failed promotion leaves the live plane untouched. That property is the
   hot-swap crash guarantee by construction.
4. Protected core: permission enforcement, event-log integrity, escalation,
   and the kill switch are never swappable by the agent. Their invariant
   checks run from outside every swappable plane.
5. Soul persistence: identity, goals, decisions, memory, and behavioral
   profile live in the session log and the memory store — outside all swapped
   code. Context compiles fresh from those stores on every resume or upgrade;
   transcripts are never carried forward raw.

## Dependency substrate (`core-deps/`)

| Directory | Role in NIO | Seam |
|---|---|---|
| `core-deps/dataworm/` | Introspection engine: self-model as a typed link graph; `impact` blast radius before any self-edit; `plan_edit` dry-run gate for change proposals; Reflex Arc change journal as hot-reload trigger | MCP server |
| `core-deps/openmem/` | Soul substrate: tiered memory consolidation, reflection loop, outcome-grounded improvement queue, recall/context/profile reconstruction on resume | MCP server |
| `core-deps/ai-sdk/` | Model router: multi-provider routing, retries, circuit breaker, security guards behind `dsh-llm`'s Service Definition | JSON-RPC gateway sidecar — **built**: `F:\deepseek-harness-master\nio-gateway` (OpenAI-compatible HTTP facade; see its README) |

All three run as supervised local sidecars (subprocess/jobs ownership, health
checks); a sidecar crash costs a worker, never the orchestrator.

Auto-sync (shipped): both MCP rows launch through
`core-deps/run-synced.mjs` → `core-deps/sync.mjs`. Every run applies any
deferred swap, then checks the pinned GitHub branch with a TTL cache
(`DW_COREDEPS_TTL_HOURS`, default 6); `DW_COREDEPS_MODE=auto|notify|off`
(auto default) decides whether latest `main`/`dev` is applied before the
sidecar spawns. Runtime-local state (`.venv/`, `data/`, root `config.json`,
compiled `.pyd`s, graph DBs) is guard-protected, deletions are
manifest-scoped, failed smoke gates roll back from `.backup/`, and swaps
blocked by running processes defer to the next boot. Pins live in
`core-deps/sync-state.json`; `bun run coredeps:{check,update,status}` drive
the same engine manually ([decision](../../../../../.agents/notes/implemented/process/2026-08-24-core-deps-auto-sync.md)).
ai-sdk source updates still need an explicit nio-gateway rebuild.

## Components to build

| # | Component | Purpose | Status |
|---|---|---|---|
| 1 | Introspection / self-model | dataworm graph of own source + plugin registry view | Not started |
| 2 | Hot-swap runtime | quiesce → atomic rebind → soak → promote/rollback over effect/disposer registry | Not started |
| 3 | Self-tasking meta-queue | feature/fix proposals become contracted tasks with dataworm gates | Not started |
| 4 | Blue/green self-replacement | new instance replays events, leases hand off with fencing tokens | Not started |
| 5 | Soul / context shifter | openmem-backed context reconstruction across resumes, upgrades, agent synthesis | Not started |
| 6 | Crash tracing + auto-postmortem | durable crash capture; diagnosis tasks feed the meta-queue automatically | Not started |
| 7 | Safety envelope | protected core checks, kill switch, canary/auto-rollback, immutable audit | Not started |

## Build order

- [x] Phase 0 — design + dependency snapshots (`core-deps/`, this preset)
- [ ] Phase 1 — reliable runtime baseline (event store, checkpoints) verified against the running dsh host
- [x] Phase 2 — mount dataworm MCP; index this repository; expose impact/plan_edit as tools (dev-local: machine-fallback paths, store at the crawler's per-root `<repo>/.dataworm/graph.db` — converged, refreshed by crawl/watch, `DW_DATAWORM_DB` overrides; 10 `mcp__dataworm__*` tools registered via `dsh-mcp-client`; first live in-session exercise against a populated store still pending an API-key run)
- [x] Phase 3 — mount openmem MCP; session-log ingestion pipeline (**done, dev-local**: 6 `mcp__openmem__*` tools mounted and verified; `ingest-openmem.py` beside this file loaded 5,945 memories from 246 real dsh transcripts into `nio-graphs/openmem-lancedb` — idempotent via content-hash ids, provenance `[dsh-session <id> seq <n>]`; recall verified through MCP. Runs in keyword-fallback retrieval mode until the embedding extra is installed; a memory written in one server process is visible to recall from the next server session onward)
- [ ] Phase 4 — ai-sdk gateway behind `dsh-llm` (**streaming landed, verified via gated mock provider**: OpenAI chunk framing incl. role-on-first-delta, reasoning passthrough, tool-call deltas, terminal usage/finish + `[DONE]`; non-streaming unchanged; `NIO_GATEWAY_MOCK=1` enables keyless self-test. **Open:** one live round-trip with a real provider key, and tool-call request content arrays)
- [ ] Phase 5 — config/prompt/skill-tier self-modification through the meta-queue
- [ ] Phase 6 — plugin-level hot-swap, journal-triggered, impact-scoped (safety envelope must land first)
- [ ] Phase 7 — blue/green orchestrator replacement with lease/fencing handoff

Each phase widens NIO's write access to itself, gated by demonstrated
reliability at the tier below.

## Reference

Architecture spec: `Autonomous Agent Orchestration Harness v1.0` (user
document; §90 durability, §92 leases/fencing, §96 approval gates, §130
no-uncontrolled-self-modification are load-bearing for NIO).
