---
description: "The workspace-scoped retrieval service contract for lexical, semantic, and hybrid search over files and session events."
kind: "package-reference"
---

# @deepseek-ai/dsh-retrieval

English | [中文](README.zh.md)

## Summary

`dsh-retrieval` defines one workspace-scoped retrieval service: consumers index documents into a project collection and search them with lexical, semantic, or hybrid ranking. It owns the request and result records, stable error codes, and the `ctx.retrieval` service key; a provider such as [`dsh-retrieval-zvec`](../retrieval-zvec/README.md) owns the medium. Use it when a consumer needs ranked search over project content without depending on one storage engine.

## Table of Contents

- [Use this package](#use-this-package)
- [Understand the implementation](#understand-the-implementation)
- [Further Exploration](#further-exploration)
- [Model Experience](#model-experience)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="use-this-package"></a>
## Use this package

Mount a provider that registers `ctx.retrieval`; this package is never mounted alone. Consumers then call one of five operations:

| Operation | Result |
|---|---|
| `indexDocuments(request)` | Upserts documents by stable id into one workspace collection |
| `removeDocuments(request)` | Removes documents by id list or metadata filter |
| `search(request)` | Returns one ranked page with an optional continuation cursor |
| `stats()` | Reports open workspace collections and total documents |
| `close()` | Closes every collection and releases provider resources |

A search request names the absolute workspace root, the query text, `lexical`/`semantic`/`hybrid` mode, a page limit, optional metadata filters, optional cursor, and optional caller vector. The provider returns `RetrievalHit[]`; each hit preserves its source scope and optional path, session id, event sequence, type, surface, and time. Query text is data, never executable index syntax.

### Filters

`RetrievalSearchFilter` ANDs the fields it receives: source scope, workspace paths, session ids, event types, event surfaces, and an inclusive event-time range. Providers translate filters into their own query language; the service rejects a reversed time range and invalid values with `RETRIEVAL_INVALID_FILTER`.

### Failures

Every failure is a typed `RetrievalError` with one of `RETRIEVAL_ABORTED`, `RETRIEVAL_INVALID_FILTER`, `RETRIEVAL_INVALID_QUERY`, `RETRIEVAL_INVALID_REQUEST`, `RETRIEVAL_PROVIDER_FAILED`, `RETRIEVAL_SEMANTIC_DISABLED`, or `RETRIEVAL_WORKSPACE_NOT_FOUND`. Cancellation is a per-call `AbortSignal`; a provider that cannot reach quiescence on abort still settles its owned promise before rejecting.

### Writing a provider

A provider owns the medium and the lifecycle. It must:

- validate the workspace root and create its data inside that project;
- accept only configured vector dimensions and reject unsupported semantic requests loudly;
- emit cursor values opaque to callers, bound to the same normalized request;
- keep accepted writes ordered and await `flush()` before reporting durability;
- make `close()` idempotent and drain accepted writes before releasing the medium.

-----

<a id="understand-the-implementation"></a>
## Understand the implementation

<details>
<summary>Implementation internals — click to expand</summary>

This section explains the design decisions behind the contract and points at the code that realizes them.

- **Service, not store.** The package owns vocabulary and the `ctx.retrieval` seam. A provider owns collections, indexing, ranking, and persistence; consumers never reach a provider-specific engine.
- **Workspace-scoped by request.** Every operation names its workspace root, so one provider instance serves several projects without a process-global current workspace.
- **Semantic is explicit.** Lexical search must work with no embedding model; semantic and hybrid ranking require configured vectors or a configured local embedder.

### Source map

| File | Role |
|---|---|
| [`src/index.ts`](src/index.ts) | The abstract `RetrievalService`, request and result records, error codes, and the `ctx.retrieval` declaration |

</details>

-----

<a id="further-exploration"></a>
## Further Exploration

- [Storage subsystem](../../../docs/subsystems/storage.md) — the related KV and domain persistence seam.
- [dsh-retrieval-zvec](../retrieval-zvec/README.md) — the shipped zvec provider.
- [dsh-tool-retrieval](../tool-retrieval/README.md) — the model-facing workspace search and index tools.

-----

<a id="model-experience"></a>
## Model Experience

Indirectly, through `dsh-tool-retrieval`, which renders these hits and failures as bounded workspace tool results.

#### KV Cache effect

No direct invalidation; the named consumer owns any request-prefix changes.

## Known Limitations and Deferred Work

<a id="known-limitations-and-deferred-work"></a>

- **Single-page cursor contract** — the service defines an opaque string cursor but does not standardize its payload; providers own cursor validity and stale handling.
- **No batch transaction** — each operation is independently durable; the service has no cross-workspace transaction or rollback.
- **Provider-owned dimensions** — the service does not validate vectors itself; a provider must reject mismatched dimensions before writing.

<a id="dev-note"></a>
### Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

None.

</details>
