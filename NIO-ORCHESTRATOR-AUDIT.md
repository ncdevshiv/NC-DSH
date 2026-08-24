# NIO Orchestrator — Trace & Audit Report

**Date:** 2026-08-24 · **Auditor:** NIO (self-audit under direct human instruction) · **Method:** static code trace (5 parallel read-only subagents) + executed live verification of every runtime-testable claim in a real session running the audited preset.

**Scope:** the `NIO Orchestrator` preset (`apps/cli/config/agent-presets/nio/`): `preset.yml`, `agent.cordis.yml`, `BLUEPRINT.md`, and every package it mounts.

---

## 1. Claim surface (what NIO claims)

From its own config and persona text, NIO claims to be *"an autonomous orchestration agent"* that carries objectives *"to verified completion"*, with:

1. **Goals** — durable same-session completion goals; create/get/update lifecycle; automatic continuation rounds bounded by `max_goal_rounds`; pause/resume/complete/blocked semantics; authority checks ("rejects non-human and subagent authority").
2. **Subagent delegation** — background-by-default delegation returning durable ids; parent settle notices; same-conversation continuation via queued messages; depth-restricted addressing; interrupt = current-turn-only.
3. **One-shot fork children** (`subagent_fork`) — deliberately non-continuable (note: `.agents/notes/implemented/architecture/2026-08-10-fork-children-stay-one-shot.md`).
4. **Workflows** — model-authored JS over worker threads: `agent()` with restricted JSON-Schema validation, `pipeline()` without inter-stage barriers, `parallel()` barrier with null-on-throw, `phase()`/`log()`/`args`, foreground execution, concurrency caps.
5. **Background jobs** — list/output(cursor)/kill lifecycle with in-session finish notices.
6. **Ralph** — fresh-agent iterative loop on explicit request only, shared workspace as memory, bounded reports across rounds (config cap 64).
7. **Skills, plan mode, ask-user, todo, web search, fs tools** — standard-mode surface.
8. **MCP sidecars** — dataworm (introspection graph) and openmem (memory store), boot-survivable when missing.
9. **Charter** — evidence rule ("never present activity as completion"), blast-radius-first self-modification discipline.

## 2. Live-executed verification (this session)

Every result below is an executed tool record from this session log.

| # | Claim | Test performed | Result |
|---|---|---|---|
| L1 | `create_goal` persists goal, returns id/rev/round-cap | created audit goal | ✅ `goal-3a3752a1…`, rev 1, activation `armed` |
| L2 | `get_goal` exact readback | read back | ✅ identical fields incl. `roundsStarted`, cap 8 |
| L3 | `update_goal` optimistic concurrency | edit w/ revision 999 | ✅ rejected: "stale goal ref … current is … revision 2" |
| L4 | arg/action validation | `blocked` carrying `objective` | ✅ rejected: "objective and max_goal_rounds are valid only with action edit" |
| L5 | `blocked` min-round gate | premature `blocked` at `roundsStarted: 0` | ⚠ **accepted** — gate is scoped to `authority.kind === 'goal-round'` only (see F1) |
| L6 | `resume` re-arm after disarm | resume rev 2 | ✅ rev 3, phase `active`, activation `armed` |
| L7 | round accounting | automatic continuation started | ✅ `roundsStarted` 0→1 exactly at round start |
| L8 | job start → output → status marker | bg `pwsh-1` echo job | ✅ both lines + `[status: completed, exit code: 0]` |
| L9 | in-session finish notice | passive | ✅ runtime notice arrived after completion |
| L10 | `job_list` finished+running | listed | ✅ pwsh-1 completed, pwsh-2 running, kinds shown |
| L11 | `job_kill` mid-run + settle | killed 120 s sleeper | ✅ immediate ack; `[status: killed, killed before exit]`; final line never printed |
| L12 | subagent durable id, background default | spawned probe child | ✅ id returned immediately |
| L13 | settle notice w/ final message | passive | ✅ `NIO-LIVE-TEST-OK 391` delivered via notice |
| L14 | `send_message` continuity (queued next turn) | follow-up referencing prior answer | ✅ `QUOTIENT-23` (=391/17) — child reused own prior output |
| L15 | `interrupt_agent` no-op on finished agent | interrupt probe child | ✅ accepted cleanly |
| L16 | `list_agents` statuses | snapshot mid-run | ✅ five `running` tracers + `ready` probe; labels present |
| L17 | workflow over worker threads | probe script | ✅ `par:["alpha","beta",null]` (throw→null barrier), `pipe:[3,5,7]` (no-barrier stages), schema-validated `child:{ok:true,…}`, JSON return surfaced verbatim; 1-agent run reported |
| L18 | dataworm MCP sidecar | `worm_summary` | ✅ live graph: 25,974 nodes / 1,714,939 edges over 13 roots |
| L19 | openmem MCP sidecar | `stats` | ✅ 5,946 memories; `embedder_available:false` matches BLUEPRINT's disclosed keyword-fallback mode |
| L20 | preset composition honesty (win32) | this session's tool catalog | ✅ `pwsh` mounted, `bash` row disabled-and-absent; codex/claude-code provider rows absent as configured |
| L21 | ralph fresh-agent loop | bounded probe, maxRounds 1 | ✅ worker had no conversation seed (knew only the objective text); structured bounded report `{status, summary, evidence, nextSteps, blocker}` crossed rounds; clean termination at round cap |
| L22 | skill catalog → instruction injection | loaded `dsh-prose-standard` via `skill` | ✅ full `<skill_content>` block injected with base-directory resolution header |
| L23 | web search availability | one trivial query | ❌ environment gap: fails loud — "DeepSeek search has no API key for DEEPSEEK_API_KEY; store it through the credentials service …" (see F2) |

**Operational incident (orchestrator, not harness):** during testing I mis-addressed a queued message to tracer `b22655e0…` believing it was a probe child that had never actually been started (batch-dispatch slip). Recovery used the same documented machinery: corrective `send_message` queued behind the stray one, instructing the tracer to ignore it. Logged as evidence that mis-addressed delegation is recoverable but not prevented — cross-checking `list_agents` labels before addressing is on the orchestrator, not the runtime.

## 3. Static code-trace findings

> Sections 3.1–3.5 are populated from the five read-only tracing subagents; verdicts there cite file+line evidence.

### 3.1 Goal subsystem — TRACED (tracer b934d24a; executed evidence: `bun run test packages/goal/tool-goal/tests/tool-goal.spec.ts` → 23/23 passed)

Architecture: domain `dsh-goal` (whole-snapshot `goal/change` events in the owning session log, strict replay fold with canonical-field validation, CAS mutations, process-local `activation`, projection unit v4, independent stream invariant); tools `tool-goal` (`get/create/update` + policy section + execution-time authority); auto-continuation via `goal-round-driver` (race-fenced one-round reservations, durability checkpoint before each round prompt, fail-closed block/disarm on anomalies, byte-match invariant over durable round prompts); human `/goal` command. NIO preset mounts only the model-facing tools into its scope; service/driver/command ride every host profile.

| Claim | Verdict | Decisive evidence |
|---|---|---|
| create persists same-session goal; rejects non-human/subagent authority | ✅ VERIFIED | authority = exact-live-agent + initiator identity + open-turn window + runtime-root membership + `user/message` with `{kind:'user'}` attestation; forged plugin-source message rejected AND real user-turn inside a child agent rejected (`tool-goal.spec.ts:212-239`) |
| get returns id/revision/objective/phase/rounds/cap/blocker/armed | ✅ VERIFIED | field-complete `GoalView` projection; real-transcript snapshot shows exactly these fields mid-run |
| edit/pause/resume need direct top-level human request | ✅ VERIFIED | `requireDirectHuman` gates each branch; steering user-message inside a goal round still qualifies (same turn window) |
| complete/blocked allowed during continuation; blocked min-round gate scoped to goal-round authority | ✅ VERIFIED (matches F1) | completion authority = direct-human OR exact admitted goal round (goalId+revision+round match); threshold default 3 enforced only for goal-round callers; direct-human bypass pinned (`tool-goal.spec.ts:620-640`) |
| Optimistic concurrency | ✅ VERIFIED | domain CAS throws GOAL_STALE_REVISION; replay fold independently enforces revision+1; live proof L3/L4 this session |
| Auto-continuation bounded; cap behavior; objective immutability | ✅ VERIFIED | driver refuses round N+1 past cap and durably self-blocks `round-limit`; stale reservation after human `edit` detected and re-driven from new revision; objective immutable to the model during autonomous rounds, mutable by human mid-run |
| Resume/fork disarm → re-arm via action=resume | ✅ VERIFIED | `agent/session-start` listener disarms (resume emits it); fresh service seeds disarmed; fork children disarmed; late-loaded driver disarms all live agents; resume re-arm human-gated, requires round-budget capacity |
| Durability | ✅ VERIFIED | JSONL append w/ fsync batches + torn-tail repair; strict fold re-validates on reload; e2e proves event lands in on-disk log without process-local activation; checkpoint failure between rounds disarms rather than proceeding unanchored |

Gaps found by the tracer: F14 (stale doc claim), F15 (read gating undocumented in description), F16 (silent driver-side terminal stops), F17 (minor cluster).

### 3.2 Subagent delegation — TRACED (tracer 14b2de33; executed evidence: `bun run test packages/subagent/tool-subagent-control/tests/tool-subagent-control.spec.ts` → 14/14 passed)

Architecture: per-provider instances of `dsh-tool-subagent` (NIO: `subagent`=spawn/continuable, `subagent_fork`=fork/one-shot), global control tools (`send_message`/`interrupt_agent`/`list_agents`), child-scoped `report` tool installed only into continuable children. Core is `SubagentContinuationManager` over an Activation residency graph: continuable start reserves a UUID childId (= the child's durable `SessionId`, persisted as JSONL session header + versioned `subagent/descriptor` event), returns immediately on inbox acceptance; settlement writes a `subagent-settled` notice directly into the parent's inbox (idle→followup turn, busy→steer at step boundary, teardown→non-waking inject) *before* ownership release. Cold resume re-authorizes against the persisted header's `parentSession`.

| Claim | Verdict | Decisive evidence |
|---|---|---|
| Background default; durable id; child stays addressable; `run_in_background:false` waits | ✅ VERIFIED | default `?? options.continuable`; returns `{kind:'continuable', subagentId}` w/o awaiting the turn; foreground path awaits result + disposes; pinned `tool-subagent.spec.ts:1113-1173` |
| Settle notice carries outcome + final message | ✅ VERIFIED (windows → F12) | `notifySettlement` builds summary + closing message; delivery ordering before ownership release tested; batched multi-child steps |
| `send_message` queues FIFO behind open turn; failure ⇒ not delivered | ✅ VERIFIED | reservation deleted + rethrown on send failure → isError; "own later turn, never steering inside the first" pinned |
| `list_agents`: live statuses, ready=storage-only resumable, pre-order descendants w/ parent+depth | ✅ VERIFIED | `statusOf` reads live Agent registry; explicit-stack pre-order sorted createdAt→id; depth-1 send_message rule enforced service-side (`authorizeLineage` requires exact direct parent; deeper fails loudly UNAUTHORIZED) |
| `interrupt_agent` current-turn-only, queue parked, descendants keep running, finished=no-op | ✅ VERIFIED | cancel with `keepInbox:true`; parked requests replayed FIFO on waking send (durable turn-end sequence pinned); grandchildren untouched; absent target returns silently |
| Fork stays one-shot | ⚠ PARTIAL (F11) | NIO compliant; three other shipped presets contradict the note |
| Depth limits | ✅ VERIFIED | `delegationDepth + 1 > maxDepth` throws; default cap 3, stamped durably so resumed children don't reset to top-level |
| "Durable id" meaning | ✅ VERIFIED | id = child SessionId UUID; JSONL header carries parentSession/origin/depth/seedLength; cold resume folds descriptor from the child log |

Gaps found by the tracer: F11 (fork-mode preset contradiction), F12 (notice loss windows), F13 (minor: interrupt no-op gives no typo signal; transient `[idle]` skew during accepted-send window; one-shot children invisible to discovery).

### 3.3 Workflow engine — TRACED (tracer 67b8e0f3; executed evidence: `bun run test packages/workflow/workflow-worker-thread/tests/session.spec.ts` → 27/27 passed)

Architecture: `tool-workflow` (consumer) → `dsh-workflow` Service Definition (`WorkflowEngine.start`; `WorkflowError` fatal-by-default) → `workflow-worker-thread` provider (meta/body validation; ONE fresh env-scrubbed `node:worker_threads` Worker per run with empty `execArgv`; tagged-JSON protocol; first-wins terminal claim; grace-timer force-settle + terminate). Worker compiles `(async () => { … })()` as a classic `vm.Script` in a bare context exposing exactly six globals + `args` (`runtime.ts:90-113`).

| Claim | Verdict | Decisive evidence |
|---|---|---|
| Plain JS / meta-as-parameter / top-level await / JSON return | ✅ VERIFIED | TS-only syntax fails pre-parse SCRIPT_PARSE (`runtime.ts:94-96`); `export const meta` regex-rejected with pointed error (`index.ts:54-67`); unserializable returns → RESULT_UNSERIALIZABLE (`realm.ts:78-151`) |
| `agent()` restricted schema, null-on-failure, option whitelist | ✅ VERIFIED | keyword set exactly `{type, oneOf, properties, required, additionalProperties, items, enum, const}` (+ignored annotations), no pattern/format (`core/tools/json-schema.ts:76-86`); failed child→null pinned (`integration.spec.ts:83-98`); `effort/isolation/agentType` named-rejected (`runtime.ts:368-374`) |
| `pipeline` no-barrier, item-null-on-throw | ✅ VERIFIED | per-item independent stage advance, catch→null (`runtime.ts:443-457`; pinned `session.spec.ts:366-387`) |
| `parallel` barrier, throw→null | ✅ VERIFIED | `Promise.all` + catch→null, fatal rethrow (`runtime.ts:413-424`) |
| Hook misuse kills script | ✅ VERIFIED | fatal-default `WorkflowError` + unforgeable host-side instanceof; forged fatal object stays null (pinned `session.spec.ts:366-387`) |
| Caps enforced; foreground run | ✅ VERIFIED | FIFO semaphore default `min(16, cores−2)`; AGENT_CAP 1000 runaway backstop; ITEM_CAP 4096; tool awaits `run.result` (`tool-workflow/src/index.ts:304`) |
| Throw/timeout/crash → tool result | ✅ VERIFIED | error mapping to isError (`tool-workflow.spec.ts:271-280`); sync-slice vm timeout 5 s (`session.spec.ts:304`); worker death → child reap + settle (`workflow-worker-thread.spec.ts:1271-1438`) |
| "No fs/network/timers" as *isolation* | ⚠ PARTIAL | absence-based containment in an intentionally escapable vm; repo's own `ESCAPE` test reaches `process.env` (`workflow-worker-thread.spec.ts:35,564-591`); documented as "containment rather than a security boundary" (`index.ts:3-5`, `realm.ts:6-7`) — see F3 |

Gaps found by the tracer: F3 (vm containment), F4 (no wall-clock timeout for async scripts), F5 (phase-matching wording), F6 (minor prose drifts).

### 3.4 Background jobs + ralph — TRACED (tracer e8c5c354; executed evidence: `vitest run packages/workflow/tool-ralph/tests/integration.spec.ts` → 8/8 passed in 4.54s)

Architecture (jobs): producers (`tool-bash` kind `bash`, `tool-pwsh` kind `pwsh`, `tool-terminal` kind `pty-send`, one-shot background `subagent`) register hooks `{cancel, done→outcome, readOutput?}` into `LocalJobRegistry` (`packages/jobs/jobs-local`), model-facing controller `tool-jobs` (`job_output`:302-340 / `job_list`:342-360 / `job_kill`:362-401) with attachController gate and settle→inject/followup notice path. Cursor state lives in producer closures (`stdoutOffset`/`stderrOffset`, `bash-local/src/index.ts:289-293`). Architecture (ralph): fixed script executed via workflow worker-thread engine; `requireFreshProvider` rejects `inheritsParentContext` providers; each round spawns a seed-less fresh child (own random session id); sole cross-round state is the prior round's size-bounded structured report (16,384-char default, validated script-side and re-checked parent-side).

| Claim | Verdict | Decisive evidence |
|---|---|---|
| `job_list` running+finished w/ ids/kinds/statuses | ✅ VERIFIED | no status filter in registry list; render `${id} [${kind}] ${status} — ${label}`; mixed-state test pinned (`tool-jobs.spec.ts:364-378`) |
| Stream cursor returns only new output; final-output idempotent | ✅ VERIFIED | "one consuming cursor" contract (`jobs/types.ts:85-90`); offsets advanced per read (`bash-local:289-293`); consuming-delta tests (`executor.spec.ts:187-198,349-360`) |
| `[status: …]` marker always appended, truncation-safe | ✅ VERIFIED | `statusLine` + `fitWithSuffix` (`tool-jobs:103-107,242-256`) |
| Non-blocking unless `wait:true`; timeout capped | ✅ VERIFIED | `Math.min(requested, waitCap)` with load-validated defaults 30 s/10 min (`tool-jobs:333,48-53`) |
| Kill: immediate ack, later killed settlement | ✅ VERIFIED (reason forwarding ⚠ see F7) | sync cancel → `stopping` → `'requested'` (`jobs-local:215-228`); live proof L11 this session |
| In-session finish notices; durable across resume | ✅ VERIFIED (edge F9) | settle→announce last; busy→inject / idle→followup w/ wake budget ≤3; inbox mutations are durable events (`core/agent/inbox.ts:186`) |
| Orphan handling on owner dispose / service dispose | ✅ VERIFIED | scope-drain `'owner disposed'`, throwing-teardown force-fail + warn (`jobs-local:459-531`; tests :664-919) |
| Ralph fresh children, no transcript seed | ✅ VERIFIED + executed | spawn provider `inheritsParentContext:false`; driver creates seed-less agent w/ fresh UUID session; integration spec proved zero parent/history markers in child requests over the REAL stack (ran green) |
| Shared workspace memory; bounded report crossing rounds | ✅ VERIFIED | script-side `validateReport` size cap + parent-side re-validation; defaults 16,384 chars; truncation-with-marker tests |
| Termination on complete/blocker/round-limit | ✅ VERIFIED | distinct renders per case incl. budget-limit text; round-failure throws carrying last durable handoff; run always disposed |
| maxRounds escalation bounded | ✅ VERIFIED | model value clamped by config ceiling (code default 256; presets set 64); above-ceiling throws |
| Ralph gated to explicit human request **in code** | ⚠ PARTIAL (F8) | gating exists only as pinned prompt text (`tool-ralph:407-411`); no code verifies request origin |

Gaps found by the tracer: F7 (kill reason dropped by real producers), F8 (ralph gating is prompt-convention only), F9 (dangling post-restart notice vs memory-only job record), F10 (minor jobs race/stall/cursor edges).

### 3.5 Auxiliary surface — TRACED (tracer b22655e0; executed evidence: `bun run test -- packages/plan/plan-mode/tests/plan-mode.spec.ts` → 63/63 passed)

| Row | Verdict | Decisive evidence |
|---|---|---|
| Skills (catalog + injection) | ✅ VERIFIED | layered discovery roots w/ ranks (`skill-filesystem/src/index.ts:246-254`); nearest-scope-wins merge; `skill` returns full body verbatim; a durable pre-step reminder literally instructs loading before acting (`tool-skill/src/index.ts:266`) — matches live proof L22 |
| Plan mode | ⚠ PARTIAL (F18/F19) | entry/exit state machine durable via `plan/mode` events; exit gates: plan-mode-only, `#` heading, review channel, Approve/Keep-planning; but the mutation prohibition is prompt-level only — no runtime code blocks mutation tools while active |
| todo allowParallelInProgress | ✅ VERIFIED | config-required boolean; description clause swaps; hard throw ">1 in_progress" when disabled |
| web fetch:false + timeout | ✅ VERIFIED | no `web_fetch` schema nor mention when false; searchTimeoutMs enforced by the tool-call timeout-policy guard reading `timeoutMs` |
| ask-user | ✅ VERIFIED (+D3) | answers return as tool result; decline = ASK_CANCELLED error; no seam-level timeout budget (bounded only by turn abort) |
| Compaction group 8192/4096/1024 | ✅ VERIFIED | values validated, equal package defaults exactly, wired through pruneSession on overflow/pressure; replay-safe replace events preserve model-visible⟺logged invariant (`docs/architecture.md:96`) |
| persona + agent-instructions maxBytes | ✅ VERIFIED (+D4) | maxBytes enforced with byte-budget fitting in render pipeline; applies to instructions only — persona text is uncapped |
| Shell rows `!!js` platform gating | ✅ VERIFIED (+D6) | disabled rows never init a fiber ⇒ zero model-facing tools; per-platform evaluation pinned in `windows-shell.spec.ts`; nuance: lazy eval at entry init/update, not YAML parse |
| MCP sidecars boot-survival | ✅ VERIFIED | `failOnStartupError` default false; missing binary ⇒ warn, reconnect attempts, loud give-up "tools unregistered", zero `mcp__*` tools, boot survives — pinned `apply.spec.ts:241,:256` |
| isolate-realm collision claim | ✅ VERIFIED | group-local realm symbols; mount audit rejects leaked root-realm services ("must sit behind an `isolate` realm or move to the host composition"); two sessions share one preset without collision (tested) |

Aux gaps: D1–D7 folded into F18–F20 below; NOT FOUND items: no vendored loader tests directory; the preset header's "plane ownership matches standard exactly" claim was not diffed against `standard/agent.cordis.yml` by the tracer.

## 4. Findings

### F1 — `blocked` min-round gate is authority-scoped; tool description reads unconditionally (doc defect, code coherent)

Tool description states: *"blocked is rejected before the configured minimum round count."* Implementation (`packages/goal/tool-goal/src/index.ts:299-303`) applies the gate only when `authority.kind === 'goal-round'` (automatic continuations; default `blockedAfterConsecutiveRounds: 3`, zod-validated positive safe integer at :33). A `blocked` from a *direct human turn* bypasses the round minimum by design (`src/authority.ts:107`: complete/blocked require a human turn or the current goal round). Executed proof: L5 accepted at round 0. The code's rule is coherent; the description's unconditional phrasing misled even the orchestrator operating it. **Recommendation:** scope the sentence to automatic continuations.

### F2 — Web search is mounted but non-functional in this deployment (environment gap, fail-loud correct)

`tool-web` is configured `fetch: false` + `searchTimeoutMs: 60000` and the tool is present in the model surface, but executing a search returns an explicit credential error naming three remediation paths (credentials service / env / literal config apiKey). The capability claim ("web search") therefore holds only where `DEEPSEEK_API_KEY` is provisioned. The failure path itself follows the repo's fail-loud convention: no silent empty result, no fabricated output.

### F3 — Workflow sandbox is documented containment, not a security boundary (honest, but claim wording should say so)

The tool description's "no filesystem, network, timers, or Node.js APIs are provided" is accurate as *provision* (bare vm realm, six injected globals, dynamic `import()` unavailable) but the realm is intentionally escapable via `globalThis.constructor.constructor('return process')()` — the repo's own suite does exactly this and reads `process.env` (`workflow-worker-thread.spec.ts:35,564-591`). Code comments, docs, and Agent Note all state "containment rather than a security boundary" on an explicit trust premise ("same trust as the model's bash access"). A hostile script escaping gains process-wide authority; mitigations are hygiene-grade (env scrub to TMP/TEMP on win32, fresh unpooled worker, terminate-on-grace). Real isolation is a documented deferred non-goal. **Verdict: implementation honest, description under-specified** — recommend the tool description carry the containment caveat.

### F4 — Workflow runs have no wall-clock timeout

Only the initial synchronous slice is bounded (5 s default). An async script parked on a promise no hook owns waits indefinitely until someone cancels the parent step (then grace-timer force-settles `cancelled` and terminates the worker). Documented deferred non-goal, but "the run executes in the foreground … returns when the whole script finishes" reads unconditional.

### F5 — `meta.phases` matching is observational, not enforced

Tool description: "Optional phase declarations matched by phase() calls". Runtime: phases are shape-checked once and never consulted again; undeclared titles emit normally, declared-but-unentered ones never appear. Intentional ("progress vocabulary only", `workflow/src/types.ts:25-26`, `docs/subsystems/workflow.md:41`) but the wording invites a validation reading.

### F6 — Minor drifts around workflow authoring contract

(a) Schema subset is a superset of the advertised "ONLY" list: annotation keywords (`description/title/default/examples`) accepted-and-ignored (`json-schema.ts:86,259-266`). (b) Agent Note claims `args` "is cloned again before exposure"; the single `workerData` structured clone already isolates the caller and no second copy exists (`runtime.ts:106-107`). Both harmless behaviorally.

### F7 — `job_kill` reason is dropped by most producers

Tool description: reason is "recorded in the log and forwarded to the job". Reality: bash/pwsh producers `cancel: () => void proc.kill()` and terminal ignores the parameter; only the subagent kind forwards it as an abort reason (`tool-subagent:424`). Nothing logs it server-side; persistence is indirect (the session log's tool-call args). Generic-producer forwarding is pinned by test, masking that shipped producers discard it.

### F8 — Ralph's explicit-human gating is prompt convention only

The claim "Use only when the direct human explicitly asks" lives solely in pinned prompt text; no code verifies request origin. Bounded escalation *is* code-enforced: model-supplied `maxRounds` throws above the config ceiling (code default 256; NIO preset 64), plus engine `maxTotalAgents` backstop.

### F9 — Post-restart notice/record mismatch for jobs

An unclaimed job-completion notice persists durably in the agent inbox, but the job record itself is memory-only and dies with the process. After a resume, `job_output <id>` on such a notice throws `unknown job`. Recovery exists (re-run or inspect via other means) but the notice's instruction becomes a dead end.

### F10 — Minor background-job edges

(a) The per-job consuming output cursor can be stolen by any non-model reader of `ctx.jobs.read()` — bytes taken are never re-delivered to `job_output`; the api-proxy pins a reads==0 guard because "the failure is invisible at the call site". (b) Teardown awaits settlement unboundedly when a producer's cancel returns without settling (throwing cancels are force-failed; silent ones are not bounded). (c) Kill-vs-completion race window: a natural finish settling first reports `completed` despite the kill request (first-wins settle).

### F11 — Fork one-shot invariant contradicted by three shipped presets (NIO unaffected)

Agent Note 2026-08-10-fork-children-stay-one-shot.md (#2124) states every shipped composition binds `subagent_fork` to one-shot. Code honors it in NIO/base bundle/examples — but the `standard`, `code`, and `cordis` CLI presets mount fork as `backgroundMode: continuable`, reintroducing exactly the prefix-reuse loss the note bans (report tool + section materialize ahead of the inherited transcript). Nothing fails loud by design; no assembled-composition test boots those presets. This is the note's own "accepted risk" realized in-tree.

### F12 — Settlement-notice delivery holds only within a live host

Crash between child settlement and parent claim loses the notice permanently (child's own session remains the durable record); a parent disposing before claiming durably cancels the notice and sees none after resume. A resumed parent recovers via `list_agents` + `send_message`, not replayed notices. Tool wording ("the runtime sends the parent a notice") is unconditional but accurate only within one host lifetime.

### F13 — Minor delegation edges

Interrupting an unknown/settled id returns accepted-no-op without a typo signal (uniform by design); `list_agents` can transiently show `[idle]` during the accepted-send admission window; one-shot children are intentionally invisible in discovery (their outcomes travel via foreground results/jobs).

### F14 — Stale documentation: the "omitted source resolves to user" defaulting does not exist

`authority.ts:66-69` JSDoc and `tool-goal/README.md:23` claim `followup`/`steer` assign a `user` source when the caller omits one. Current code has no such defaulting: `UserMessage.source` is required and forwarded as-is. Security posture unaffected (mandatory sources mean nothing silently inherits human authority), but the described mechanism is fictional.

### F15 — `get_goal` is read-gated like a mutation, undocumented in its description

All three goal tools require live-agent + initiator identity + running status + an open turn; calls before/after the turn are rejected (pinned). The README documents it under Authority; the tool description ("Read the current same-session goal") does not.

### F16 — Driver-initiated terminal states stop silently from the transcript's perspective

When the round driver self-blocks (`round-limit`, `queue-failed`, `prompt-rejected`) it writes only the durable `goal/change`; the `<goal_blocked>` closing-message injection exists solely in the tool path. Durable state and UI carry the signal; the conversation just stops.

### F17 — Minor goal-machinery edges

`command-goal` collapses every `GoalError` into one generic message (hides stale-revision race causes from the human); tool-schema numerics accept any `number` at the edge with structured rejection later (per repo boundary convention); "model must justify persistence of the blocking condition" is advisory-only by design (disclosed in the package README).

### F18 — Plan-mode mutation prohibition is prompt-level only

The preset text says plan-mode rules "override any later tool description … those tools remain listed"; no runtime code blocks or guards mutation tools while plan mode is active — sandbox/approval stacks explicitly do not read plan state (`plan-mode/src/index.ts:4-7`). Documented-as-designed and consistent with the "tool catalog stays unchanged" rationale, but users should know enforcement is model discipline, not mechanism. exit_plan_mode approval gates only the mode switch, not mutations.

### F19 — ask_user_question has no timeout budget

No `timeoutMs` ⇒ the cooperative timeout policy passes through; the question lives until turn abort/cancel. Decline semantics (ASK_CANCELLED) live in UI providers, not the seam.

### F20 — Minor auxiliary edges

Persona text has no byte cap (only agent-instructions does); NIO pruner values exactly duplicate package defaults (explicit-but-redundant); `!!js` tags evaluate lazily at entry init/update rather than YAML parse (platform flips re-evaluate on reload); missing-sidecar behavior retries reconnects before the loud give-up; no vendored loader tests directory exists and the preset header's plane-parity claim vs `standard/` was not diffed.

## 5. Verdict

## 5. Verdict

**NIO's orchestration claims are real.** Every capability the preset claims — goals with authority-checked lifecycle and bounded auto-continuation, continuable subagent delegation with settlement notices and lineage enforcement, one-shot fork, model-authored workflows over worker threads with schema validation and caps, the background-job cursor/kill/notice lifecycle, the fresh-agent ralph loop, skills, plan mode, todo/web/fs/shell tools, and both MCP sidecars — traces to concrete, code-enforced implementation, and every runtime-testable claim passed executed live verification in this session (§2: L1–L23) plus five focused test runs executed during tracing (23/23 goal authority, 14/14 delegation control, 27/27 workflow session, 8/8 ralph integration, 63/63 plan mode). No fabricated or mock-backed capability was found.

**Findings tally (F1–F20):** zero critical enforcement failures; one environment gap (F2, web search needs `DEEPSEEK_API_KEY`); documentation-clarity defects where wording promises more than code scopes (F1 blocked-gate scope, F5 phase matching, F14 stale source-defaulting claim, F15 undocumented read gating); honest-but-understated design boundaries worth promoting into tool descriptions (F3 escapable vm containment, F12 notice loss windows, F4 no workflow wall-clock timeout); prompt-level-only enforcement disclosed as designed (F8 ralph gating, F18 plan-mode mutation ban); one cross-preset config regression contradicting an implemented Agent Note (F11, `standard`/`code`/`cordis` fork mode — NIO unaffected); assorted minor edges (F6, F7, F9, F10, F13, F16–F20).

**Caveats:** this was a self-audit — the auditor is the audited system, run under a direct human instruction; the delegation incident in §2 (mis-addressed message, recovered via queued correction) is logged as evidence that address hygiene rests on the orchestrator, not the runtime. Static verdicts cite file+line evidence read by independent tracing subagents; live rows cite tool records from this session log.
