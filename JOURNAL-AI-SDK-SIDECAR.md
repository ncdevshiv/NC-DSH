# AI SDK sidecar migration journal (single LLM adapter)

Append-only chronological journal for replacing `@deepseek-ai/dsh-llm-deepseek`
and `@deepseek-ai/dsh-llm-pi-ai` with one adapter
(`@deepseek-ai/dsh-llm-ai-sdk`) driving the multi-provider AI SDK at
`F:\alisia\ai-sdk` through a single stdio JSON-RPC `ai-sidecar` child process.

Rules set by the owner, 2026-08-25:
- Entries are never removed or rewritten; new entries are appended.
- The AI SDK itself is expected to contain errors, missing features, partial
  implementations, stubs, placeholders, and TODOs. Any such finding must be
  traced to root cause and fixed in the AI SDK and/or the harness — never
  ignored, suppressed, or blindly removed.
- The harness work is done but not committed; commit happens only on explicit
  say-so.

Entry format per section: Wanted / Had / Did / Errors + root causes / options
considered / chosen solution + why / files edited / expected goal next /
test + review.

---

## 2026-08-25 (backfill) — sidecar built, adapter wired, old packages deleted

*Backfill entry reconstructing work already completed and verified in earlier
sessions of this effort, recorded here so the journal starts complete.*

### Wanted
1. One LLM adapter for every provider route: DeepSeek official, OpenRouter,
   Ollama, Anthropic, Gemini, OpenAI-compatible gateways.
2. Nothing else in the harness speaks a provider wire protocol.
3. The multi-provider capability comes from the AI SDK at `F:\alisia\ai-sdk`,
   reached over one stdio JSON-RPC child process (`ai-sidecar`).

### Had (starting point)
- Two in-house adapters: `packages/llm/llm-deepseek` (DeepSeek HTTP) and
  `packages/llm/llm-pi-ai` (`@earendil-works/pi-ai` multi-provider), each with
  its own wire code, config shape, credential handling, and composition rows.
- The AI SDK Rust workspace at `F:\alisia\ai-sdk` existed without a process
  wrapper the harness could drive.

### Did
1. Built the Rust sidecar at `F:\alisia\ai-sdk\crates\ai-sidecar`:
   - stdio JSON-RPC framing, `configure` / `provider.list` /
     generation-request methods.
   - 6 protocol tests pass (`cargo test`).
   - Release binary at `target/release/ai-sidecar.exe`.
   - Live smoke over the real binary: configure → provider.list succeeded.
2. Wrote the harness adapter `packages/llm/llm-ai-sdk`:
   - Implements `LlmAdapter` for every registered route.
   - Per-request credential resolution, settings hot-replace, sidecar
     re-configure on generation change, image-byte cap, model cap on
     attachment bytes, per-route default thinking effort.
   - Lifecycle dispose via Cordis effect.
3. Rewired composition: `packages/bundle/base/cordis.patch.yml`, 49 example
   YAMLs (script-transformed), python/sdk-runtime, packages/sdk/server,
   tsconfig.base.json, tsconfig.host.json, vitest.config.ts, knip.json,
   examples/package.json. Old llm-deepseek / llm-pi-ai rows removed everywhere.
4. Deleted old packages: `git rm -r packages/llm/llm-deepseek
   packages/llm/llm-pi-ai .github/workflows/pi-ai-provider-e2e.yml`.
5. Swept all UI references: packages/client, packages/host, apps/web TS files;
   fixed a plugin-catalog duplicate-key bug introduced by that sweep.
6. Rewired tests importing the adapters: llm-retry,
   session-title-first-prompt-llm, subagent-spawn-in-process,
   agent-instructions, agent-loop, tool-fs, apps/cli/tests,
   headless-agent tests — now import the new package and use the new
   providers config shape.

### Errors / root causes / fixes
- **Sidecar lifecycle bug: responders lost on EOF.** Caught during protocol
  testing; fixed in the sidecar so pending responders are not dropped when
  the stream ends. (Root cause detail lives with the sidecar commits; noted
  here because it was found by the harness-facing test loop.)
- **Plugin-catalog duplicate-key bug** introduced while sweeping UI files.
  Fixed during the sweep.

### Test / review
- Typecheck net change from this work: −5 errors (stale package references),
  new package source clean. 10 remaining repo errors are pre-existing in
  ui-home-section / ui-team-section / ui-work-section (`t` prop /
  GoalProjection) and web/error-body.ts — untouched by this migration.
- Sidecar: 6/6 cargo protocol tests green; live configure/provider.list smoke
  green against `target/release/ai-sidecar.exe`.

### Expected goal next
Regenerate catalogs, update prose docs + THIRD_PARTY_NOTICES, write the Agent
Note, run the real e2e with `DSH_AI_SDK_SIDECAR` pointed at the release
binary, then `bun run hygiene` + `bun run doc-sync`. Commit only on say-so.

---

## 2026-08-25 (session continuation) — remaining gates begin

### Wanted
Finish the six outstanding items above; keep the journal live as each lands.

### Had
All code, composition, and test rewires complete (see backfill). Docs still
referenced the two deleted adapters. No journal existed yet for this effort.

### Did
1. Created this journal (separate from the Bun.open `JOURNAL.md`, which stays
   intact and untouched).

---

## 2026-08-25 (session continuation) — catalog generators, doc sweep, UI leftover found and triaged

### Wanted
1. Regenerate every generated doc that encodes the package graph and config schema.
2. Update all prose docs, READMEs, THIRD_PARTY_NOTICES, and Agent Notes that described the two deleted adapters.
3. Trace any migration leftover to root cause; fix or record honestly.

### Had
English `docs/config-catalog.md` still carried the deleted `dsh-llm-deepseek` section. The curated seam tables in `scripts/gen-doc-graphs.ts` still named llm-deepseek/llm-pi-ai, so regenerated graph docs re-emitted them. The zh counterparts of generated docs were stale. The new package had NO README at all (the repo contract requires one, with a Model Experience section). The web Models page's custom-provider flow was built on the removed per-route `api` field.

### Did
1. Added JSDoc prose to the six `AiSdkCatalogModel` fields (gen-config-catalog rejects fields without prose); `bun run gen-config-catalog` then wrote the new catalog section.
2. Updated the curated `SERVICE_ROLES` table in `scripts/gen-doc-graphs.ts` (attachments consumers → llm-ai-sdk; llm implementations → [llm-ai-sdk, llm-replay]; settings/credentials consumers → [llm-ai-sdk, apiproxy]); regenerated module-graph, capability-seams, and the four composition.md docs (`gen-doc-graphs` wrote 8 docs; `gen-module-graph` 1).
3. Mirrored both zh graph docs: capability-seams.zh.md by hand-editing nodes/edges/rows; module-graph.zh.md by rebuilding its body from the new generator output (its maintenance model is generated body + translated header), which also picked up earlier drift (browser group, moli/searxng, ui-home/team/work rows).
4. Wrote `packages/llm/llm-ai-sdk/README.md` + `README.zh.md`: config table, provider-route/dialect table, request assembly, dynamic configuration, sidecar lifecycle, errors, Model Experience, Known Limitations.
5. Rewrote `docs/user/guide/providers.md` + zh around the new reality: sidecar binary setup first, default DeepSeek route, route-id→dialect table, settings.yaml declarations, hard image cap, reasoning controls, updated troubleshooting.
6. Fixed source + pastes together: dsh-llm `LlmAdapter` JSDoc no longer mandates attribution headers on adapters whose transport another process owns; updated the ```ts public-api paste in llm-streaming.md + zh identically.
7. Re-pointed settings.md's `validate` example from pi-ai to bash-local's real `assertServiceableBashConfig`.
8. Updated cookbooks (adding-an-llm-adapter, extension-cookbook), practice page (llm-adapter), group/package READMEs (packages/llm, llm, llm-retry, anonymous-user-id, sdk/server, agent-spine-demo, jsonrpc-agent examples) in both languages.
9. Rewrote `ui-settings-models` README pair for current behavior.
10. Regenerated THIRD_PARTY_NOTICES via `bun run gen-third-party-notices` — `@earendil-works/pi-ai` and `eventsource-parser` dropped automatically. No hand-added row for ai-sidecar: it is an external, user-built binary not distributed by this repo, and the notices file is byte-gated by its generator.
11. Wrote the superseding Agent Note triplet `2026-08-25-single-llm-adapter-via-ai-sdk` (+zh +i18n sidecar) and added "superseded in part" pointers to twin-llm-adapters, provider-routed-llm-adapters, and mandatory-app-attribution-headers (+zh each). Archived notes untouched.
12. Re-recorded translation-pairing sidecars for every edited pair.

### Errors / root causes / options
- **gen-config-catalog failed with 6 violations** — new catalog model fields had no JSDoc prose. Root cause: adapter written before running the generator. Fix: documented the fields against dsh-llm's conventions.
- **Regenerated graph docs still showed deleted packages** — root cause was NOT the generator logic but the curated tables inside `scripts/gen-doc-graphs.ts`. Fix: update curated facts, regenerate.
- **Models page "Add a custom provider" permanently disabled** — `protocolChoices()` probes `providers.<route>.api`, which the new ProviderProfile schema dropped; it returned `[]`, gating the create button off forever. `layoutOf` also carried a dead duplicate `llm-ai-sdk` branch returning `'pi-ai'`. Options considered: (a) rewrite the card without a protocol field AND implement draft-endpoint discovery through the sidecar; (b) leave the entry point disabled, document settings.yaml declaration as the path, record the redesign as deferred work; (c) delete the card outright. Chose (b) now plus removing the provably-dead branch: (a) requires new sidecar protocol surface for interrogating unsaved endpoints (profiles without keys are skipped by configure) — real feature work deserving its own design; (c) destroys reusable UI for that future work. Recorded in ui-settings-models README Known Limitations and the Agent Note.
- **sdk/server README rewrite initially wrong** — I first claimed "no automatic mounting"; code shows `initialize` still auto-mounts a fallback, now `LlmAiSdk`. Corrected both languages immediately after verifying against `server.ts`.

### Files edited
scripts/gen-doc-graphs.ts; packages/llm/llm-ai-sdk/src/adapter.ts; packages/client/ui-settings-models/src/client/ProviderEditor.tsx; packages/llm/llm/src/index.ts; generated: docs/config-catalog(.zh).md, docs/module-graph(.zh).md, docs/capability-seams(.zh).md, apps/cli/composition.md, examples/*/composition.md, THIRD_PARTY_NOTICES.md; prose pairs: providers, adding-an-llm-adapter, extension-cookbook, llm-adapter practice, settings, llm-streaming; READMEs: llm-ai-sdk (new ×2), packages/llm ×2, llm ×2, llm-retry ×2, anonymous-user-id ×2, sdk/server ×2, ui-settings-models ×2, agent-spine-demo ×2, jsonrpc-agent ×2; notes: single-llm-adapter-via-ai-sdk triplet (new), three supersession pointer pairs + sidecars.

### Test / review
Pairing re-recorded per pair; formal gates deferred to the end of session (verify-config-catalog, verify-doc-graphs, verify-md-links, verify-doc-budgets, verify-agent-note-format/classification, verify-type-equiv).

### Expected goal next
Run the real e2e with DSH_AI_SDK_SIDECAR; then hygiene + doc-sync; fix whatever they surface.

---

## 2026-08-26 (session) — outstanding gates land: discovery shipped, Models page completed, sidecar lifecycle closed

### Wanted
1. Run the real e2e with `DSH_AI_SDK_SIDECAR` pointed at the release binary.
2. Run `bun run hygiene` + `bun run doc-sync`; fix whatever they surface.
3. Owner mandate for this session: no stubs/mocks/hardcoding/partial work anywhere in the surface, frontend included — finish the deferred custom-provider flow end to end.

### Had
All code, composition, docs, and test rewires from the earlier entries. Three links missing for the disabled entry point: the sidecar had no way to interrogate an unsaved endpoint, the adapter registered no model-discovery handler, and the schema carried no per-route wire-protocol field for the UI to read. Separately, the owner's machine had accumulated hundreds of orphaned `node.exe` processes.

### Did
1. **Sidecar lifecycle bug fixed** (`packages/llm/llm-ai-sdk/src/sidecar.ts`): the spawned child was never stored on the client, so `dispose()` could not kill it and a failed initialize cleared the spawn memo while leaving a healthy process behind — every test run or plugin reload leaked live processes, and a timed-out initialize respawned a second sidecar beside the first. The child is now retained before any await; teardown is one idempotent, generation-aware path shared by exit, spawn failure, failed initialize, and disposal; launch failures reject with their own wording instead of transport noise; a new readonly `pid` accessor gives tests the observation seam. Unit tests pin both kill-on-dispose and kill-on-failed-initialize plus fresh-respawn by polling OS pid liveness against real spawned children.
2. **Wire dialects**: `ProviderProfile.api?` (`openai-compatible | anthropic | google`) added to config/schema/ResolvedRoute and passed through `configure`; omission keeps exact id-based derivation (`defaultApiOf`). The AI SDK gained `AiClientBuilder.provider_as` so configure registers adapters under the ROUTE name (two routes may share one format), and `create_provider_with_api` selects native adapters for explicit dialects while rejecting unknown ones loud before any network call. Release binary rebuilt.
3. **Draft-endpoint discovery** (`model.discover`, protocol v1): transient provider per interrogation, never joining the configured generation; OpenAI-compatible drafts without a base URL refuse at the contract level. Harness: `AiSidecarClient.discoverModels` + `ctx.llm.registerModelDiscovery('llm-ai-sdk', …)` — known route with nothing new in the draft answers from its advisory catalog with zero network; otherwise dialect resolves draft → route profile → route-id table, stored credential backs a keyless draft, wire rows map to `LlmDiscoveredModel` at the boundary, and the one-shot key is held nowhere.
4. **Models page completed** (`packages/client/ui-settings-models`): `protocolChoices()` reads the restored schema union, enabling the create card exactly when the namespace mounts; hand-declared routes edit displayName + api through the same fields the create card asked for; every route's model list edits through the interrogation-aware editor. Fixed three product bugs the specs exposed: settings attach re-derives the directory through the registration's atomic `replace` handle (a bare re-registration collided with already-declared ids and tore the whole settings wiring down mid-attach — this is why hot-reload never landed); ModelListEditor reset clears keystroke buffers/expanded rows; the add card forwards the directory's `declared` flag and customized composition-owned routes get their Reset control. Two stale locale keys removed (en+zh).
5. **Composition fixture modernization**: ui-settings-models fixtures rewritten from the two-adapter museum to one current-shape namespace; home/team/work spec types fixed (locale seat imports); python/development zh section reordered to mirror EN.
6. **Real-binary e2e extended** (`tests/adapter.e2e.ts`): local scripted OpenAI-compatible server now answers `GET /models`; second case drives `ctx.llm.discoverModels` through composition → adapter → REAL release sidecar → live HTTP listing, asserting endpoint order and that the stored credential crossed as a bearer token.
7. **Docs/gates**: llm-ai-sdk README(+zh) documents `api`, the derivation table override, model discovery, and generation-aware lifecycle; config catalog regenerated (+zh mirrored); Agent Note triplet `2026-08-26-model-discovery-and-route-dialects` written with pairings re-recorded; superseded pi-ai-era facts updated in place across five active notes; type-equiv manifest/doc synced for the migrated `providerType` field; knip rows updated (deleted packages → llm-ai-sdk, client-section patterns, desktop preload ignore, uv binary).

### Errors / root causes / fixes
- **Settings hot-reload silently dead** (found by the composition suite while landing discovery): root cause was the duplicate directory declaration inside `onChange` — see (4). Diagnosed via fiber-trace instrumentation of registration commit/dispose.
- **Route-key collision in the AI SDK** (caught by my own Rust test): `configure` with a custom id + native dialect registered under the adapter's id, breaking `route:model` resolution; fixed with `provider_as`.
- **Dialect contract mismatch between ends** (caught by real-binary e2e): harness sent explicit `openai-compatible`, Rust treated only `None` as the family default; aligned so naming the family explicitly selects the same adapter.
- **Windows host privilege limits (environmental, not code)**: `settings-file` symlink persistence test and `verify-node-next-types`' symlinked-consumer typecheck require OS symlink privilege (admin/Developer Mode) this host lacks; both are healthy on CI/Linux and left untouched.

### Test / review
- `bun run vitest run packages/llm/llm-ai-sdk packages/client/ui-settings-models packages/client/ui-home-section packages/client/ui-team-section packages/client/ui-work-section packages/settings packages/llm/llm`: green (683+ passing; sole failure is the committed Windows-symlink privilege test above).
- `DSH_AI_SDK_SIDECAR=<release> bun run vitest run --config vitest.e2e.config.ts packages/llm/llm-ai-sdk`: 2/2 green (stream + unsaved-endpoint discovery through the real binary).
- `bun run typecheck`: 0 errors repository-wide (the previously noted 10 were these packages' own uncommitted drift, now fixed).
- `bun run doc-sync`: 28/28 gates pass. `bun run knip`: clean. Remaining hygiene sub-gate `verify-node-next-types` blocked only by the Windows symlink-privilege environment above.
- Still uncommitted by standing rule; cloud real-API e2e remains key-gated (`DEEPSEEK_API_KEY`) and self-skips locally.

### Expected goal next
Owner review of the diff; provide `DEEPSEEK_API_KEY` for one recorded cloud-e2e run if desired; then commit on say-so.
