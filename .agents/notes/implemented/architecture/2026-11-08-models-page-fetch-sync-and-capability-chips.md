# Agent Note: Models page fetches sync the served list and state capabilities

Status: implemented

## Problem

Fetching a provider's model list adopted every model the endpoint reported and never removed one. An endpoint that retired a model left the stored row selected forever, so the picker offered a model the provider no longer serves and a request to it failed at dispatch. The same reply also discarded the capability facts it carried: a user could not tell from the list whether a model reasons or accepts images without opening its options or reading `cordis.patch.yml`.

Removing stale rows cannot simply delete everything the reply omits. A person adds models by hand for endpoints that do not list them, and a deployment may have written rows before any provenance existed; deleting those would destroy configuration on the first fetch.

## Decision

### Fetched rows record their provenance

An adopted row stores `source: 'discovered'`; a row added through **Add model** stores `source: 'custom'`. A row with no marker predates the field and is treated as endpoint-owned: a fetch stamps it `discovered` when the reply names it and removes it when the reply does not, so an existing provider starts matching its endpoint on the first sync rather than keeping retired models forever. The field is declared on each adapter's model entry schema, so it is part of the section contract the settings service projects rather than an unknown key; serving and dispatch never read it.

### One fetch syncs the owned list

`mergeDiscoveredModels` keeps every owned row whose id the reply still names or whose provenance is `custom`, prunes every other owned row, and appends every reported model nothing serves yet. A row that is still reported keeps its stored fields, including capacities and input types a person corrected, so a sync never overwrites a measurement. The result carries `added` and `removed`, and the banner names both counts. A reply that names nothing leaves the list untouched: a route serving no model cannot resolve, so pruning every row would break it.

A catalog route keeps serving its installed catalog from the layer beneath, which no fetch writes; pruning only addresses the owned list, so catalog inheritance and its per-model adjustments stay intact.

### Discovery states reasoning capability

`LlmDiscoveredModel` gains an optional `reasoning` flag. The pi-ai catalog path reports the installed entry's own `reasoning`; a wire listing discloses no such fact and omits it. Absence means unknown, not false: a surface states only what someone measured.

### Rows state what was disclosed

Each model row shows capability chips under its identity: **Thinking** when the model reasons, **Vision** when it accepts image input, and its context window. The row's own declaration supplies a fact when the listing disclosed none, and a hand-added row also carries the **Custom** tag. A fact neither source states stays unmentioned rather than guessed.

## Alternatives considered

- **A per-row Keep toggle.** Rejected: it makes every fetch an explicit curation pass and still cannot classify rows written before the toggle existed.
- **Overwrite every reported row with the latest values.** Rejected: it discards a capacity a person corrected against a provider whose listing is wrong, which is the case the editor exists for.
- **Treat an unmarked row as custom.** Rejected: every row stored before provenance existed would survive every fetch, so a provider would keep offering retired models forever — the defect this note addresses. A fetch is an explicit sync, and rows added after this change carry `custom`.
- **Derive tool-use capability from a listing.** Rejected: no listing in scope discloses it, so the chip would restate an assumption.

## Consequences

- A retired model leaves the list on the next fetch, and the banner says how many rows moved in each direction.
- A hand-added model added after this change survives every fetch until the user deletes it.
- A provider whose list predates provenance is adopted by its first fetch: surviving rows are stamped `discovered`, so later syncs need no inference.
- A provider whose listing discloses reasoning or image input now shows those facts without opening the row.
- A reply that names nothing keeps the current rows and reports the empty listing.

## Verification

- `packages/client/ui-settings-models/tests/provider-form.client.spec.tsx` covers pruning stale rows from a provider configured before provenance existed and from one that records it, keeping a custom row, stamping surviving rows, reporting added and removed counts from both the editor and the provider row, dropping a pruned id from `modelOrder`, and leaving the list intact when the endpoint lists nothing.
- `packages/client/ui-settings-models/tests/model-list-catalog.client.spec.tsx` covers the capability chips derived from a listing and from a row's own declaration, the **Custom** tag, a removal-only sync, and the empty-sync guard.
- `packages/client/ui-settings-models/tests/components.client.spec.tsx` pins the marker written by the DeepSeek editor's **Add model**.
- `packages/llm/llm/tests/topology.spec.ts` covers the discovery passthrough of `reasoning`, including absence.
- `packages/llm/llm-pi-ai/tests/discovery.spec.ts` pins the catalog path's reasoning facts against the installed entries.
- `packages/llm/llm-pi-ai/tests/config.spec.ts` and `packages/llm/llm-deepseek/tests/catalog-provenance.spec.ts` pin the marker surviving each adapter's settings schema.
