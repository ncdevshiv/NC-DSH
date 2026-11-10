---
description: "Package map for workspace retrieval: the zvec-backed lexical, semantic, and hybrid search service with model-facing tools and a live session-event indexer."
kind: "package-group"
---

# retrieval/ — workspace retrieval capability family

English | [中文](README.zh.md)

## Summary

The `retrieval/` group gives a project its own ranked search over files and session text. A service contract defines lexical, semantic, and hybrid retrieval; one provider implements it with the native zvec engine and stores every collection, log, and model cache inside the workspace; a model-facing tool pair exposes search and indexing; and a consumer mirrors live session events into the same collection. Lexical search works with no embedding model, so retrieval is active by default without network access or model downloads. This page maps the group; each package README owns its per-package contract.

## Table of Contents

- [Packages](#packages)
- [Related documentation](#related-documentation)
- [Dev Note](#dev-note)

-----

<a id="packages"></a>
## Packages

| Package | Role | ctx key |
|---|---|---|
| [`retrieval/`](retrieval/README.md) | Workspace retrieval service definition: records, modes, filters, errors | `ctx.retrieval` |
| [`retrieval-zvec/`](retrieval-zvec/README.md) | Native zvec provider: repository-local collections, FTS, dense vectors, optional local embeddings | registers `ctx.retrieval` |
| [`retrieval-session-index/`](retrieval-session-index/README.md) | Mirrors committed live session events into the workspace collection | — |
| [`tool-retrieval/`](tool-retrieval/README.md) | Model-facing `workspace_search` and `workspace_index` tools | `ctx.tools` |

-----

<a id="related-documentation"></a>
## Related documentation

- [Retrieval subsystem](../../docs/subsystems/retrieval.md) — the type-level contract, modes, and repository-local storage rules.
- [Session-query family](../session-query/README.md) — exact history reads and the SQLite full-text search backend.
- [Storage subsystem](../../docs/subsystems/storage.md) — the separate non-session KV persistence seam.
- [zvec upstream](https://github.com/alibaba/zvec) — the engine behind the shipped provider.

The `dsh-base` bundle mounts `retrieval-zvec`, `tool-retrieval`, and `retrieval-session-index` by default with embeddings disabled, so a shipped profile gets native workspace full-text search without downloading a model.

-----

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

None.

</details>
